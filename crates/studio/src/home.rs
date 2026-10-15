//! The K2 Press folder and this app's settings.
//!
//! The library is the earlier app's folder, read in place: nothing is copied, so the web app and
//! this one can run side by side. Its location comes from (first wins) `K2_PRESS_HOME`, the saved
//! setting, or a `K2_PostTool` folder next to (or above) the executable or the working directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, StudioError, err};

/// This app's own settings (`%APPDATA%\K2 Press Desktop\studio.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The K2 Press folder; empty = discover.
    pub home: String,
    /// `hermes` or `ollama` (or any `llm.backends` name); empty = config.yaml's.
    pub backend: String,
    /// Model override; empty = the backend's.
    pub model: String,
    /// Run the scheduler here (publishes due calendar posts). Off by default so the web app's
    /// scheduler and this one never both send.
    pub run_scheduler: bool,
    /// Last brand picked in the panel.
    pub brand: String,
    /// Cloud Instagram account per brand (brand key → cloud account id); unset = matched by
    /// the brand's Instagram id.
    pub cloud_accounts: std::collections::BTreeMap<String, String>,
}

pub fn settings_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("K2 Press Desktop")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(settings_dir().join("studio.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let dir = settings_dir();
        std::fs::create_dir_all(&dir)?;
        write_atomic(&dir.join("studio.json"), serde_json::to_string_pretty(self)?.as_bytes())
    }
}

/// Write via a temp file and rename, so a crash never leaves half a file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("x")));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        StudioError(format!("could not save {}: {e}", path.display()))
    })
}

/// A K2 Press folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Home {
    pub root: PathBuf,
}

fn looks_like_home(p: &Path) -> bool {
    p.join("config.yaml").is_file() || p.join("config.example.yaml").is_file()
}

impl Home {
    pub fn new(root: impl Into<PathBuf>) -> Result<Home> {
        let root = root.into();
        if !looks_like_home(&root) {
            return err(format!("{} has no config.yaml — pick your K2 Press (K2_PostTool) folder", root.display()));
        }
        Ok(Home { root })
    }

    /// Find the folder (see the module docs).
    pub fn discover(settings: &Settings) -> Result<Home> {
        if let Some(h) = std::env::var_os("K2_PRESS_HOME") {
            return Home::new(PathBuf::from(h));
        }
        if !settings.home.trim().is_empty() {
            return Home::new(settings.home.trim());
        }
        let mut starts: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            starts.extend(exe.ancestors().skip(1).take(5).map(Path::to_path_buf));
        }
        if let Ok(cwd) = std::env::current_dir() {
            starts.extend(cwd.ancestors().take(4).map(Path::to_path_buf));
        }
        for s in starts {
            for c in [s.join("K2_PostTool"), s.clone()] {
                if c.join("config.yaml").is_file() && c.join("library").is_dir() {
                    return Ok(Home { root: c });
                }
            }
        }
        err("K2 Press folder not found. Set it in Post Studio ▸ Settings (the folder with config.yaml), or set K2_PRESS_HOME.")
    }

    pub fn config_path(&self) -> PathBuf {
        let c = self.root.join("config.yaml");
        if c.is_file() { c } else { self.root.join("config.example.yaml") }
    }
    pub fn library(&self) -> PathBuf {
        self.root.join("library")
    }
    pub fn outputs(&self) -> PathBuf {
        self.root.join("outputs")
    }
    pub fn image_cache(&self) -> PathBuf {
        self.root.join("image_cache")
    }
    /// A config-relative path (`static/logo.png`) made absolute.
    pub fn resolve(&self, rel: &str) -> PathBuf {
        let p = Path::new(rel);
        if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) }
    }

    /// `.env` merged under the process environment (the environment wins, as in python-dotenv).
    pub fn env(&self) -> Env {
        let mut vars = parse_env(&std::fs::read_to_string(self.root.join(".env")).unwrap_or_default());
        // Cloud credentials: the K2 Press .env first, else the Shorts Factory Desktop library's
        // .env (same Supabase backend), read in place.
        if !vars.contains_key("CLOUD_SUPABASE_URL")
            && let Some(sf) = shorts_factory_env()
        {
            for (k, v) in sf.into_iter().filter(|(k, _)| k.starts_with("CLOUD_")) {
                vars.entry(k).or_insert(v);
            }
        }
        for (k, v) in std::env::vars() {
            if vars.contains_key(&k)
                || k.starts_with("K2_")
                || k.starts_with("META_")
                || k.starts_with("CLOUD_")
                || k.ends_with("_API_KEY")
                || k == "PUBLIC_BASE_URL"
            {
                vars.insert(k, v);
            }
        }
        Env { vars }
    }
}

/// The Shorts Factory Desktop library's `.env` (`%APPDATA%\Shorts Factory Desktop\.env`), if any.
fn shorts_factory_env() -> Option<BTreeMap<String, String>> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
    let text = std::fs::read_to_string(base.join("Shorts Factory Desktop").join(".env")).ok()?;
    Some(parse_env(&text))
}

/// Settings from `.env` and the environment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Env {
    pub vars: BTreeMap<String, String>,
}

impl Env {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(|s| s.trim()).filter(|s| !s.is_empty())
    }
}

/// `KEY=value` lines; `#` comments, `export `, and single or double quotes.
pub fn parse_env(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        let mut v = v.trim().to_string();
        if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2) || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2) {
            v = v.get(1..v.len() - 1).unwrap_or("").to_string();
        } else if let Some(i) = v.find(" #") {
            v.truncate(i);
            v = v.trim().to_string();
        }
        out.insert(k.trim().to_string(), v);
    }
    out
}
