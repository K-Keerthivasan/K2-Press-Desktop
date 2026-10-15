//! Local AI: the Hermes CLI or any OpenAI-compatible server (Ollama by default), as K2 Press's
//! `llm.py`. Replies are parsed as JSON, with one retry that asks for strict JSON.

use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::Config;
use crate::home::{Env, Settings};
use crate::{Result, StudioError, err};

#[derive(Clone, Debug, PartialEq)]
pub enum Backend {
    Hermes { command: String, profile: String, model: Option<String>, timeout_s: u64 },
    OpenAi { base_url: String, model: String, api_key: String },
}

impl Backend {
    pub fn name(&self) -> String {
        match self {
            Backend::Hermes { model, .. } => format!("Hermes{}", model.as_deref().map(|m| format!(" ({m})")).unwrap_or_default()),
            Backend::OpenAi { base_url, model, .. } => format!("{model} @ {base_url}"),
        }
    }

    /// Backend names (built-ins merged with `llm.backends`).
    pub fn names(cfg: &Config) -> Vec<String> {
        let mut v = vec!["hermes".to_string(), "ollama".to_string()];
        if let Some(m) = cfg.at("llm.backends").and_then(Value::as_object) {
            for k in m.keys() {
                if !v.contains(k) {
                    v.push(k.clone());
                }
            }
        }
        v
    }

    /// The backend to use: the app setting, else `K2_LLM_*`, else config.yaml's `llm`.
    pub fn resolve(cfg: &Config, env: &Env, settings: &Settings) -> Backend {
        let preset = |name: &str| -> (String, String) {
            let (db, dm) = match name {
                "hermes" => ("hermes://cli", "hermes:gpt-5.5"),
                _ => ("http://localhost:11434/v1", "qwen3:8b"),
            };
            let p = cfg.at(&format!("llm.backends.{name}"));
            let b = p.and_then(|p| p.get("base_url")).and_then(Value::as_str).unwrap_or(db).to_string();
            let m = p.and_then(|p| p.get("model")).and_then(Value::as_str).unwrap_or(dm).to_string();
            (b, m)
        };
        let (mut base, mut model) = if settings.backend.trim().is_empty() {
            (
                env.get("K2_LLM_BASE_URL").map(str::to_string).or_else(|| cfg.str_at("llm.base_url")).unwrap_or_else(|| "hermes://cli".into()),
                env.get("K2_LLM_MODEL").map(str::to_string).or_else(|| cfg.str_at("llm.model")).unwrap_or_else(|| "hermes:gpt-5.5".into()),
            )
        } else {
            preset(settings.backend.trim())
        };
        if !settings.model.trim().is_empty() {
            model = settings.model.trim().to_string();
        }
        if base.trim().is_empty() {
            base = "hermes://cli".into();
        }
        if base == "hermes://cli" {
            Backend::Hermes {
                command: env.get("K2_HERMES_COMMAND").unwrap_or("hermes").to_string(),
                profile: env.vars.get("K2_HERMES_PROFILE").map(|s| s.trim().to_string()).unwrap_or_else(|| "k2-posttool".into()),
                // Only explicit Hermes model names; a stale Ollama name makes Hermes answer nothing.
                model: model.strip_prefix("hermes:").map(str::to_string),
                timeout_s: env.get("K2_HERMES_TIMEOUT").and_then(|t| t.parse().ok()).unwrap_or(300),
            }
        } else {
            Backend::OpenAi { base_url: base.trim_end_matches('/').to_string(), model, api_key: env.get("K2_LLM_API_KEY").or_else(|| cfg.at("llm.api_key").and_then(Value::as_str)).unwrap_or("ollama").to_string() }
        }
    }

    fn complete(&self, messages: &[(String, String)]) -> Result<String> {
        match self {
            Backend::Hermes { command, profile, model, timeout_s } => {
                let mut prompt: Vec<String> = messages.iter().map(|(r, c)| format!("{}:\n{c}", r.to_uppercase())).collect();
                prompt.push("INSTRUCTION:\nReply with only one valid JSON object. Do not use markdown.".into());
                let mut cmd = Command::new(command);
                if let Some(m) = model {
                    cmd.args(["-m", m]);
                }
                if !profile.is_empty() {
                    cmd.args(["-p", profile]);
                }
                cmd.args(["-z", &prompt.join("\n\n")]);
                run_with_timeout(cmd, Duration::from_secs(*timeout_s), command)
            }
            Backend::OpenAi { base_url, model, api_key } => {
                let msgs: Vec<Value> = messages.iter().map(|(r, c)| json!({"role": r, "content": c})).collect();
                let body = json!({"model": model, "messages": msgs, "temperature": 0.1, "response_format": {"type": "json_object"}, "stream": false});
                let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(600)).build();
                let resp = agent.post(&format!("{base_url}/chat/completions")).set("Authorization", &format!("Bearer {api_key}")).send_json(body).map_err(|e| StudioError(format!("AI server at {base_url}: {}", http_error(e))))?;
                let v: Value = resp.into_json().map_err(|e| StudioError(format!("AI server reply: {e}")))?;
                v.pointer("/choices/0/message/content").and_then(Value::as_str).map(str::to_string).ok_or_else(|| StudioError("AI server reply has no message".into()))
            }
        }
    }

    /// One structured request: system + user → a JSON value (object or array).
    pub fn chat_json(&self, system: &str, user: &str) -> Result<Value> {
        let mut messages = vec![("system".to_string(), system.to_string()), ("user".to_string(), user.to_string())];
        let mut last = StudioError("no reply".into());
        for _ in 0..2 {
            let text = self.complete(&messages)?;
            match clean_json(&text) {
                Ok(v) => return Ok(v),
                Err(e) => last = e,
            }
            messages.push(("user".into(), "Your previous reply was not valid JSON. Reply again with ONLY a single valid JSON object.".into()));
        }
        Err(last)
    }
}

fn run_with_timeout(mut cmd: Command, timeout: Duration, name: &str) -> Result<String> {
    use std::io::Read;
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up for each request.
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| StudioError(format!("could not start `{name}` ({e}). Is the Hermes CLI installed? Or switch the AI to Ollama in Settings.")))?;
    let mut out = child.stdout.take();
    let mut errp = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut s);
        }
        s
    });
    let ereader = std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(o) = errp.as_mut() {
            let _ = o.read_to_end(&mut s);
        }
        s
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return err(format!("`{name}` took longer than {}s", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let stdout = String::from_utf8_lossy(&reader.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&ereader.join().unwrap_or_default()).to_string();
    if !status.success() {
        let detail = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        return err(format!("`{name}` failed: {}", photocraft_press::text::clip(detail, 400)));
    }
    Ok(stdout.trim().to_string())
}

pub(crate) fn http_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            format!("HTTP {code}: {}", photocraft_press::text::clip(&body, 300))
        }
        ureq::Error::Transport(t) => t.to_string(),
    }
}

/// Strip `<think>` blocks and code fences; parse the first JSON object or array.
pub fn clean_json(text: &str) -> Result<Value> {
    let mut t = text.to_string();
    while let (Some(a), Some(b)) = (t.find("<think>"), t.find("</think>")) {
        if b < a {
            break;
        }
        t.replace_range(a..b + "</think>".len(), "");
    }
    let t = t.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Ok(v);
    }
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let (Some(a), Some(b)) = (t.find(open), t.rfind(close))
            && b > a
            && let Some(slice) = t.get(a..=b)
            && let Ok(v) = serde_json::from_str::<Value>(slice)
        {
            return Ok(v);
        }
    }
    err(format!("the AI did not reply with JSON: {}", photocraft_press::text::clip(t, 300)))
}
