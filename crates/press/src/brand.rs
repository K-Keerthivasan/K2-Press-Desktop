//! A channel brand: identity, colours and the AI voice. Read from the `brands:` block of the K2
//! Press `config.yaml` (by `photocraft-studio`), shared with the earlier web app.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Brand colours. Keys match `config.yaml` (`navy` is the background base).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub navy: String,
    pub navy2: String,
    pub accent: String,
    pub accent2: String,
    pub text: String,
}

impl Default for Theme {
    fn default() -> Self {
        Theme { navy: "#0A0F1E".into(), navy2: "#0C1426".into(), accent: "#00B4C8".into(), accent2: "#00C896".into(), text: "#FFFFFF".into() }
    }
}

/// One entry of a brand's Create-tab content types.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentType {
    pub value: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brand {
    /// The `brands:` key (`k2`, `jkr`, …); also the library folder name.
    #[serde(skip)]
    pub key: String,
    pub name: String,
    pub short: String,
    pub author: String,
    pub handle: String,
    pub instagram: String,
    pub tagline: String,
    pub pitch: String,
    pub services: String,
    pub location: String,
    pub website: String,
    pub email: String,
    pub category: String,
    /// As written in the config (relative to the K2 Press folder); `logo` holds the resolved path.
    pub logo_path: String,
    #[serde(skip)]
    pub logo: Option<String>,
    /// `wide` for a full wordmark (not cropped to a circle).
    pub logo_shape: String,
    /// Image-led layouts (image + text split), as JKR uses.
    pub image_forward: bool,
    pub hashtags: Vec<String>,
    pub theme: Theme,
    /// Font family for the layouts; empty = Arial.
    pub font: String,
    pub profile: String,
    pub personality: String,
    pub scoring_guidance: String,
    pub tone: String,
    pub content_types: Vec<ContentType>,
    /// Anything else in the block (feeds, video, templates…), kept so nothing is lost.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl Brand {
    /// A brand from its config block. Unknown or mistyped keys fall back to defaults rather than
    /// failing the whole config.
    pub fn from_value(key: &str, v: &Value) -> Brand {
        let mut b: Brand = serde_json::from_value(v.clone()).unwrap_or_else(|_| lenient(v));
        b.key = key.to_string();
        if b.name.is_empty() {
            b.name = key.to_string();
        }
        b
    }

    pub fn font(&self) -> &str {
        if self.font.trim().is_empty() { "Arial" } else { self.font.trim() }
    }

    /// The `@handle`, with the `@`.
    pub fn at_handle(&self) -> String {
        let h = self.handle.trim();
        if h.is_empty() {
            if self.instagram.trim().is_empty() { String::new() } else { format!("@{}", self.instagram.trim()) }
        } else if h.starts_with('@') {
            h.to_string()
        } else {
            format!("@{h}")
        }
    }

    /// Feed categories: (key, display name, enabled, urls).
    pub fn feed_categories(&self) -> Vec<(String, String, bool, Vec<String>)> {
        let Some(cats) = self.extra.get("feeds").and_then(|f| f.get("categories")).and_then(Value::as_object) else {
            return Vec::new();
        };
        cats.iter()
            .map(|(k, c)| {
                let name = c.get("name").and_then(Value::as_str).unwrap_or(k).to_string();
                let enabled = c.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                let urls =
                    c.get("urls").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                (k.clone(), name, enabled, urls)
            })
            .collect()
    }
}

/// Field-by-field read for a block that doesn't deserialise as a whole (say `hashtags: seo`).
fn lenient(v: &Value) -> Brand {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let theme = v.get("theme").and_then(|t| serde_json::from_value(t.clone()).ok()).unwrap_or_default();
    let hashtags = match v.get("hashtags") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(s)) => crate::text::split_tags(s),
        _ => Vec::new(),
    };
    Brand {
        name: s("name"),
        short: s("short"),
        handle: s("handle"),
        instagram: s("instagram"),
        tagline: s("tagline"),
        website: s("website"),
        category: s("category"),
        logo_path: s("logo_path"),
        profile: s("profile"),
        personality: s("personality"),
        location: s("location"),
        image_forward: v.get("image_forward").and_then(Value::as_bool).unwrap_or(false),
        hashtags,
        theme,
        ..Brand::default()
    }
}
