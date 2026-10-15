//! Stock and web images into `image_cache/`, as K2 Press's `images.py` (Pexels, Unsplash, any
//! image URL). Files are named the same way, so both apps reuse each other's cache.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::home::{Env, Home};
use crate::{Result, StudioError, err};

const UA: &str = "K2PressDesktop/1.0";
const MAX_IMAGE_BYTES: u64 = 40 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub thumb: String,
    pub full: String,
    pub credit: String,
    pub source: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).user_agent(UA).build()
}

fn slugify(t: &str) -> String {
    photocraft_press::text::slug(t, "image")
}

/// Search a source (`pexels` or `unsplash`) for `query`.
pub fn search(env: &Env, source: &str, query: &str, count: usize) -> Result<Vec<Hit>> {
    let q = query.trim();
    if q.is_empty() {
        return err("type what the image should show");
    }
    let count = count.clamp(1, 30);
    match source {
        "unsplash" => {
            let key = env.get("UNSPLASH_API_KEY").ok_or_else(|| StudioError("UNSPLASH_API_KEY is not set in .env".into()))?;
            let v: Value = agent()
                .get("https://api.unsplash.com/search/photos")
                .query("query", q)
                .query("per_page", &count.to_string())
                .query("orientation", "portrait")
                .set("Authorization", &format!("Client-ID {key}"))
                .call()
                .map_err(|e| StudioError(format!("Unsplash: {}", crate::llm::http_error(e))))?
                .into_json()
                .map_err(|e| StudioError(format!("Unsplash: {e}")))?;
            Ok(v.get("results").and_then(Value::as_array).into_iter().flatten().filter_map(|r| Some(Hit { thumb: r.pointer("/urls/small")?.as_str()?.into(), full: r.pointer("/urls/regular")?.as_str()?.into(), credit: r.pointer("/user/name").and_then(Value::as_str).unwrap_or("").into(), source: "unsplash".into() })).collect())
        }
        _ => {
            let key = env.get("PEXELS_API_KEY").ok_or_else(|| StudioError("PEXELS_API_KEY is not set in .env (free at pexels.com/api)".into()))?;
            let v: Value = agent()
                .get("https://api.pexels.com/v1/search")
                .query("query", q)
                .query("per_page", &count.to_string())
                .query("orientation", "portrait")
                .set("Authorization", key)
                .call()
                .map_err(|e| StudioError(format!("Pexels: {}", crate::llm::http_error(e))))?
                .into_json()
                .map_err(|e| StudioError(format!("Pexels: {e}")))?;
            Ok(v.get("photos").and_then(Value::as_array).into_iter().flatten().filter_map(|r| Some(Hit { thumb: r.pointer("/src/medium")?.as_str()?.into(), full: r.pointer("/src/large2x")?.as_str()?.into(), credit: r.get("photographer").and_then(Value::as_str).unwrap_or("").into(), source: "pexels".into() })).collect())
        }
    }
}

fn looks_like_image(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if b.starts_with(b"\x89PNG") {
        Some("png")
    } else if b.len() > 12 && b.get(..4) == Some(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else {
        None
    }
}

/// Download an image URL into the cache as `<prefix>-<name>.<ext>`; returns the path.
pub fn download(home: &Home, url: &str, prefix: &str, name: &str) -> Result<PathBuf> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return err("image links must start with http:// or https://");
    }
    let resp = agent().get(url).call().map_err(|e| StudioError(format!("image download: {}", crate::llm::http_error(e))))?;
    let mut bytes = Vec::new();
    resp.into_reader().take(MAX_IMAGE_BYTES).read_to_end(&mut bytes)?;
    let ext = looks_like_image(&bytes).ok_or_else(|| StudioError("that link is not a JPEG, PNG, WebP or GIF image".into()))?;
    let dir = home.image_cache();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{prefix}-{}.{ext}", slugify(name)));
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

/// First result for `query` from `source`, downloaded (`px-…` / `us-…` like the web app).
pub fn fetch_for_query(home: &Home, env: &Env, source: &str, query: &str) -> Result<PathBuf> {
    let prefix = if source == "unsplash" { "us" } else { "px" };
    let cached = home.image_cache().join(format!("{prefix}-{}.jpg", slugify(query)));
    if cached.is_file() {
        return Ok(cached);
    }
    let hits = search(env, source, query, 1)?;
    let hit = hits.first().ok_or_else(|| StudioError(format!("no {source} images for “{query}”")))?;
    download(home, &hit.full, prefix, query)
}

/// Copy a local file into the cache (so the library keeps working when the original moves).
pub fn import_local(home: &Home, src: &Path) -> Result<PathBuf> {
    let bytes = std::fs::read(src)?;
    let ext = looks_like_image(&bytes).ok_or_else(|| StudioError(format!("{} is not a JPEG, PNG, WebP or GIF image", src.display())))?;
    let dir = home.image_cache();
    std::fs::create_dir_all(&dir)?;
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let path = dir.join(format!("up-{}.{ext}", slugify(stem)));
    std::fs::write(&path, &bytes)?;
    Ok(path)
}
