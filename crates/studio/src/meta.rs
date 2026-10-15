//! Instagram and Facebook publishing through the Meta Graph API, as K2 Press's `meta.py`:
//! container → poll until FINISHED → publish for Instagram (Meta fetches each image from
//! `PUBLIC_BASE_URL/outputs/<rel>/<file>`), multipart uploads for Facebook Pages, a local 24-hour
//! rate guard shared with the web app (`library/meta_rate.json`), and the same queue-entry
//! bookkeeping, so nothing publishes twice and a setup problem keeps the post's slot.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::config::Config;
use crate::home::{Env, Home, write_atomic};
use crate::library::{Entry, entry_files, now_iso, s};
use crate::{Result, StudioError, err};

const IG_LOGIN_BASE: &str = "https://graph.instagram.com";
const POLL_TRIES: u32 = 20;
const POLL_DELAY: Duration = Duration::from_secs(3);

struct Account {
    brand: String,
    ig_id: String,
    page_id: String,
    token: String,
    fb_token: String,
    targets: Vec<String>,
    ig_base: String,
    fb_base: String,
    max_per_day: i64,
}

fn account(cfg: &Config, env: &Env, brand: &str) -> std::result::Result<Account, String> {
    let meta = cfg.at("meta").cloned().unwrap_or(Value::Null);
    let acct = meta
        .get("accounts")
        .and_then(|a| a.get(brand))
        .ok_or_else(|| format!("No Meta account mapped for brand '{brand}'. Add it under meta.accounts in config.yaml."))?;
    let token_env = acct.get("token_env").or_else(|| meta.get("token_env")).and_then(Value::as_str).unwrap_or("META_ACCESS_TOKEN");
    let token = env.get(token_env).ok_or_else(|| format!("{token_env} is not set — put your Meta access token in .env"))?.to_string();
    let fb_token = acct.get("fb_token_env").and_then(Value::as_str).and_then(|k| env.get(k)).map_or_else(|| token.clone(), str::to_string);
    let ver = meta.get("api_version").and_then(Value::as_str).unwrap_or("v21.0");
    let fb_base = format!("https://graph.facebook.com/{ver}");
    let ig_base = if acct.get("login").and_then(Value::as_str).unwrap_or("facebook").to_lowercase().starts_with("instagram") {
        format!("{IG_LOGIN_BASE}/{ver}")
    } else {
        fb_base.clone()
    };
    let list = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect()).unwrap_or_default()
    };
    let mut targets = list(acct.get("targets"));
    if targets.is_empty() {
        targets = list(meta.pointer("/publish/default_targets"));
    }
    if targets.is_empty() {
        targets = vec!["instagram".into()];
    }
    if targets.iter().any(|t| t == "both") {
        targets = vec!["instagram".into(), "facebook".into()];
    }
    let rl = meta.get("rate_limit");
    let max = rl.and_then(|r| r.get("max_posts_per_day")).and_then(Value::as_i64).unwrap_or(25)
        - rl.and_then(|r| r.get("safety_margin")).and_then(Value::as_i64).unwrap_or(2);
    Ok(Account {
        brand: brand.into(),
        ig_id: acct
            .get("ig_user_id")
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
            .unwrap_or_default()
            .trim()
            .trim_matches('"')
            .to_string(),
        page_id: acct
            .get("fb_page_id")
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
            .unwrap_or_default()
            .trim()
            .trim_matches('"')
            .to_string(),
        token,
        fb_token,
        targets,
        ig_base,
        fb_base,
        max_per_day: max.max(1),
    })
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(90)).build()
}

/// Meta's own error message, which is the useful half of debugging a failed publish.
fn graph_result(r: std::result::Result<ureq::Response, ureq::Error>) -> Result<Value> {
    let (code, text) = match r {
        Ok(resp) => (resp.status(), resp.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap_or_default()),
        Err(ureq::Error::Transport(t)) => return err(format!("Graph API unreachable: {t}")),
    };
    let v: Value =
        serde_json::from_str(&text).map_err(|_| StudioError(format!("Graph API returned non-JSON ({code}): {}", photocraft_press::text::clip(&text, 200))))?;
    if code >= 400 || v.get("error").is_some() {
        let e = v.get("error").cloned().unwrap_or(Value::Null);
        let msg = e.get("error_user_msg").or_else(|| e.get("message")).and_then(Value::as_str).unwrap_or(&text).to_string();
        let sub = e.get("error_subcode").map(|s| format!("/{s}")).unwrap_or_default();
        return err(format!("Meta API error {}{sub}: {msg}", e.get("code").map(Value::to_string).unwrap_or_default()));
    }
    Ok(v)
}

fn get(base: &str, path: &str, token: &str, params: &[(&str, &str)]) -> Result<Value> {
    let mut req = agent().get(&format!("{base}/{path}")).query("access_token", token);
    for (k, v) in params {
        req = req.query(k, v);
    }
    graph_result(req.call())
}

fn post(base: &str, path: &str, token: &str, params: &[(&str, String)]) -> Result<Value> {
    let mut form: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    form.push(("access_token", token));
    graph_result(agent().post(&format!("{base}/{path}")).send_form(&form))
}

fn id_of(v: &Value) -> Result<String> {
    v.get("id").map(|i| i.as_str().map_or_else(|| i.to_string(), str::to_string)).ok_or_else(|| StudioError("Graph API reply has no id".into()))
}

// ── Rate guard (shared file with the web app: {"ig:<id>": [unix seconds…]}) ──

fn rate_path(home: &Home) -> PathBuf {
    home.library().join("meta_rate.json")
}

fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn rate_stamps(home: &Home, key: &str) -> Vec<f64> {
    let v: Value = std::fs::read_to_string(rate_path(home)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let cutoff = unix_now() - 86400.0;
    v.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).filter(|t| *t > cutoff).collect()).unwrap_or_default()
}

fn rate_check(home: &Home, key: &str, max: i64) -> Result<()> {
    let n = rate_stamps(home, key).len() as i64;
    if n >= max { err(format!("Local rate guard: {n} posts published to this account in the last 24h (ceiling {max}).")) } else { Ok(()) }
}

fn rate_note(home: &Home, key: &str) {
    let mut v: Value = std::fs::read_to_string(rate_path(home)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
    let mut stamps = rate_stamps(home, key);
    stamps.push(unix_now());
    if let Some(o) = v.as_object_mut() {
        o.insert(key.into(), json!(stamps));
        if let Ok(t) = serde_json::to_string(&v) {
            let _ = write_atomic(&rate_path(home), t.as_bytes());
        }
    }
}

// ── Instagram ────────────────────────────────────────────────────────────────

fn wait_ready(base: &str, token: &str, container: &str) -> Result<()> {
    for _ in 0..POLL_TRIES {
        let v = get(base, container, token, &[("fields", "status_code,status")])?;
        match v.get("status_code").and_then(Value::as_str) {
            Some("FINISHED") => return Ok(()),
            Some("ERROR") => return err(format!("Meta rejected the image: {}", v.get("status").and_then(Value::as_str).unwrap_or("unknown error"))),
            _ => std::thread::sleep(POLL_DELAY),
        }
    }
    err("Timed out waiting for Instagram to ingest the image container.")
}

fn publish_instagram(home: &Home, a: &Account, ptype: &str, urls: &[String], caption: &str) -> Result<Value> {
    if urls.is_empty() {
        return err(
            "Instagram publishing needs public image URLs. Set PUBLIC_BASE_URL in .env to an https address that serves the K2 Press outputs folder, then republish.",
        );
    }
    if let Some(u) = urls.iter().find(|u| u.starts_with("http://localhost") || u.starts_with("http://127.0.0.1")) {
        return err(format!("PUBLIC_BASE_URL points at localhost ({u}). Meta's servers must be able to fetch the image — use a publicly reachable https URL."));
    }
    if a.ig_id.is_empty() {
        return err(format!("Brand '{}' has no ig_user_id in config.yaml.", a.brand));
    }
    let key = format!("ig:{}", a.ig_id);
    rate_check(home, &key, a.max_per_day)?;
    let (b, t, ig) = (a.ig_base.as_str(), a.token.as_str(), a.ig_id.as_str());
    let media = format!("{ig}/media");
    let container = match ptype {
        "carousel" => {
            if !(2..=10).contains(&urls.len()) {
                return err(format!("Instagram carousels take 2-10 images (got {}).", urls.len()));
            }
            let mut children = Vec::new();
            for u in urls {
                let c = id_of(&post(b, &media, t, &[("image_url", u.clone()), ("is_carousel_item", "true".into())])?)?;
                wait_ready(b, t, &c)?;
                children.push(c);
            }
            let p = id_of(&post(b, &media, t, &[("media_type", "CAROUSEL".into()), ("children", children.join(",")), ("caption", caption.into())])?)?;
            wait_ready(b, t, &p)?;
            p
        }
        "story" => {
            let c = id_of(&post(b, &media, t, &[("image_url", urls[0].clone()), ("media_type", "STORIES".into())])?)?;
            wait_ready(b, t, &c)?;
            c
        }
        _ => {
            let c = id_of(&post(b, &media, t, &[("image_url", urls[0].clone()), ("caption", caption.into())])?)?;
            wait_ready(b, t, &c)?;
            c
        }
    };
    let media_id = id_of(&post(b, &format!("{ig}/media_publish"), t, &[("creation_id", container)])?)?;
    rate_note(home, &key);
    let permalink =
        get(b, &media_id, t, &[("fields", "permalink")]).ok().and_then(|v| v.get("permalink").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    Ok(json!({"platform": "instagram", "media_id": media_id, "permalink": permalink, "post_type": ptype}))
}

// ── Facebook Pages ───────────────────────────────────────────────────────────

fn multipart_photo(base: &str, page: &str, token: &str, path: &Path, published: bool, caption: &str) -> Result<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(30 * 1024 * 1024).read_to_end(&mut bytes)?;
    let boundary = format!("----k2press{:x}", (unix_now() * 1000.0) as u64);
    let mime = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) { "image/png" } else { "image/jpeg" };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image.png").replace('"', "");
    let mut body: Vec<u8> = Vec::new();
    let mut field = |k: &str, v: &str| body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
    field("access_token", token);
    field("published", if published { "true" } else { "false" });
    if !caption.is_empty() {
        field("caption", caption);
    }
    body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"source\"; filename=\"{name}\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
    body.extend(&bytes);
    body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
    let r = agent().post(&format!("{base}/{page}/photos")).set("Content-Type", &format!("multipart/form-data; boundary={boundary}")).send_bytes(&body);
    id_of(&graph_result(r)?)
}

fn publish_facebook(home: &Home, a: &Account, ptype: &str, files: &[PathBuf], caption: &str) -> Result<Value> {
    if a.page_id.is_empty() {
        return err(format!("Brand '{}' has no fb_page_id in config.yaml.", a.brand));
    }
    if ptype == "story" {
        return err("Facebook Page stories are not supported by this build.");
    }
    if files.is_empty() {
        return err("No images to publish to Facebook.");
    }
    let key = format!("fb:{}", a.page_id);
    rate_check(home, &key, a.max_per_day)?;
    let (b, t, page) = (a.fb_base.as_str(), a.fb_token.as_str(), a.page_id.as_str());
    let post_id = if files.len() == 1 {
        multipart_photo(b, page, t, &files[0], true, caption)?
    } else {
        let mut media = Vec::new();
        for f in files {
            media.push(json!({"media_fbid": multipart_photo(b, page, t, f, false, "")?}));
        }
        id_of(&post(b, &format!("{page}/feed"), t, &[("message", caption.into()), ("attached_media", Value::Array(media).to_string())])?)?
    };
    rate_note(home, &key);
    Ok(
        json!({"platform": "facebook", "media_id": post_id, "post_type": ptype, "permalink": format!("https://www.facebook.com/{}", post_id.replace('_', "/posts/"))}),
    )
}

/// Publish one queue entry to its brand's targets; returns the web app's `publish` record:
/// `{sent, via, results, errors}` or `{sent: false, reason}` when setup is incomplete.
pub fn publish_entry_record(home: &Home, cfg: &Config, env: &Env, e: &Entry) -> Value {
    let files = entry_files(home, e);
    if files.is_empty() || s(e, "rel").is_empty() {
        return json!({"sent": false, "reason": "no assets to publish"});
    }
    if let Some(f) = files.iter().find(|f| !f.is_file()) {
        return json!({"sent": false, "reason": format!("missing image {}", f.display())});
    }
    let a = match account(cfg, env, s(e, "brand")) {
        Ok(a) => a,
        Err(reason) => return json!({"sent": false, "reason": reason}),
    };
    let base = env.get("PUBLIC_BASE_URL").unwrap_or("").trim_end_matches('/').to_string();
    let rel = s(e, "rel");
    let urls: Vec<String> = if base.is_empty() {
        Vec::new()
    } else {
        e.get("files").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(|f| format!("{base}/outputs/{rel}/{f}")).collect()
    };
    let ptype = photocraft_press::Format::from_key(s(e, "format")).post_type(files.len());
    let caption = s(e, "caption");
    let caption: String = if caption.chars().count() > 2200 { caption.chars().take(2199).collect::<String>() + "…" } else { caption.to_string() };
    let targets: Vec<String> = e
        .get("targets")
        .and_then(Value::as_array)
        .map(|t| t.iter().filter_map(Value::as_str).map(str::to_lowercase).collect())
        .filter(|t: &Vec<String>| !t.is_empty())
        .unwrap_or_else(|| a.targets.clone());
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for t in &targets {
        let r = match t.as_str() {
            "instagram" => publish_instagram(home, &a, ptype, &urls, &caption),
            "facebook" => publish_facebook(home, &a, ptype, &files, &caption),
            other => err(format!("Unknown target '{other}' (instagram | facebook).")),
        };
        match r {
            Ok(v) => results.push(v),
            Err(e) => errors.push(json!({"platform": t, "error": e.0})),
        }
    }
    let sent = !results.is_empty();
    json!({"sent": sent, "via": "meta", "ok": sent && errors.is_empty(), "results": results, "errors": errors, "at": now_iso()})
}

/// Publish and stamp the result on the entry, exactly as the web app's `_publish_entry`.
pub fn publish_entry(home: &Home, cfg: &Config, env: &Env, e: &mut Entry) -> Value {
    let was = s(e, "status").to_string();
    let pub_ = publish_entry_record(home, cfg, env, e);
    let attempts = e.get("publish_attempts").and_then(Value::as_i64).unwrap_or(0) + 1;
    e.insert("publish".into(), pub_.clone());
    e.insert("publish_attempts".into(), json!(attempts));
    e.insert("last_attempt_at".into(), json!(now_iso()));
    let sent = pub_.get("sent").and_then(Value::as_bool).unwrap_or(false);
    let errors = pub_.get("errors").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    if sent {
        e.insert("status".into(), json!("published"));
        e.insert("published_at".into(), json!(now_iso()));
        e.remove("blocked_since");
        e.remove("blocked_reason");
    } else if errors {
        e.insert("status".into(), json!("failed"));
    } else {
        // Setup is incomplete: the post never left the machine, so it keeps its slot.
        e.insert("status".into(), json!(if was == "scheduled" { "scheduled" } else { "approved" }));
        e.insert("blocked_reason".into(), pub_.get("reason").cloned().unwrap_or_else(|| json!("publishing is not configured")));
        if !e.contains_key("blocked_since") {
            e.insert("blocked_since".into(), json!(now_iso()));
        }
    }
    pub_
}

/// Check the token and account ids for a brand (Settings ▸ Test connection).
pub fn verify(cfg: &Config, env: &Env, brand: &str) -> Result<String> {
    let a = account(cfg, env, brand).map_err(StudioError)?;
    let mut lines = Vec::new();
    if !a.ig_id.is_empty() {
        let v = get(&a.ig_base, &a.ig_id, &a.token, &[("fields", "username")])?;
        lines.push(format!("Instagram: @{}", v.get("username").and_then(Value::as_str).unwrap_or("?")));
        if let Ok(q) = get(&a.ig_base, &format!("{}/content_publishing_limit", a.ig_id), &a.token, &[("fields", "config,quota_usage")]) {
            let used = q.pointer("/data/0/quota_usage").and_then(Value::as_i64).unwrap_or(0);
            let total = q.pointer("/data/0/config/quota_total").and_then(Value::as_i64).unwrap_or(25);
            lines.push(format!("{used}/{total} posts used in the last 24h"));
        }
    }
    if !a.page_id.is_empty() {
        let v = get(&a.fb_base, &a.page_id, &a.fb_token, &[("fields", "name")])?;
        lines.push(format!("Facebook Page: {}", v.get("name").and_then(Value::as_str).unwrap_or("?")));
    }
    if env.get("PUBLIC_BASE_URL").is_none() {
        lines.push("PUBLIC_BASE_URL is not set: Instagram can't fetch images until it is.".into());
    }
    if lines.is_empty() {
        return err(format!("Brand '{brand}' has neither ig_user_id nor fb_page_id."));
    }
    Ok(lines.join("\n"))
}
