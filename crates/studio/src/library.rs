//! The K2 Press library, shared in place with the web app:
//!
//! - `library/review_queue.json`: the review queue and the calendar (one list). Statuses:
//!   `pending → approved → scheduled → published`, plus `failed` and `rejected`.
//! - `library/plans/<brand>/*.json`: saved plans.
//! - `library/used_urls.json`: stories already turned into posts.
//! - `outputs/<rel>/NN_kind.png`: rendered slides (`rel` = `<brand>/<date>_<slug>_<format>`).
//!
//! Entries are kept as JSON objects so fields this app doesn't know survive a save.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::home::{Home, write_atomic};
use crate::{Result, StudioError, err};

pub type Entry = Map<String, Value>;

pub fn s<'a>(e: &'a Entry, k: &str) -> &'a str {
    e.get(k).and_then(Value::as_str).unwrap_or("")
}

pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn queue_path(home: &Home) -> PathBuf {
    home.library().join("review_queue.json")
}

pub fn load_queue(home: &Home) -> Result<Vec<Entry>> {
    let p = queue_path(home);
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| StudioError(format!("review_queue.json is damaged ({e}); fix or move it aside")))?;
    Ok(v.as_array().map(|a| a.iter().filter_map(|x| x.as_object().cloned()).collect()).unwrap_or_default())
}

pub fn save_queue(home: &Home, items: &[Entry]) -> Result<()> {
    std::fs::create_dir_all(home.library())?;
    let v = Value::Array(items.iter().cloned().map(Value::Object).collect());
    write_atomic(&queue_path(home), serde_json::to_string_pretty(&v)?.as_bytes())
}

/// Read-modify-write one entry by id.
pub fn update_entry(home: &Home, id: &str, f: impl FnOnce(&mut Entry) -> Result<()>) -> Result<Entry> {
    let mut items = load_queue(home)?;
    let e = items.iter_mut().find(|e| s(e, "id") == id).ok_or_else(|| StudioError(format!("post {id} is no longer in the queue")))?;
    f(e)?;
    let out = e.clone();
    save_queue(home, &items)?;
    Ok(out)
}

pub fn remove_entry(home: &Home, id: &str) -> Result<()> {
    let mut items = load_queue(home)?;
    let n = items.len();
    items.retain(|e| s(e, "id") != id);
    if items.len() == n {
        return err(format!("post {id} is not in the queue"));
    }
    save_queue(home, &items)
}

fn new_id() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = blake_ish(t ^ u128::from(std::process::id()));
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

fn blake_ish(mut x: u128) -> u64 {
    // SplitMix-style mixing: ids only need to be unique, not secret.
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x as u64) ^ ((x >> 64) as u64)
}

/// The output folder for a new post: `outputs/<brand>/<date>_<slug>_<format>` (unique).
pub fn new_output_dir(home: &Home, brand: &str, slug: &str, format: &str) -> Result<(String, PathBuf)> {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let brand = photocraft_press::text::slug(brand, "brand");
    let base = format!("{brand}/{date}_{}_{format}", photocraft_press::text::slug(slug, "post"));
    let mut rel = base.clone();
    let mut n = 2;
    while home.outputs().join(&rel).exists() {
        rel = format!("{base}-{n}");
        n += 1;
        if n > 999 {
            return err("too many posts with this name today");
        }
    }
    let dir = home.outputs().join(&rel);
    std::fs::create_dir_all(&dir)?;
    Ok((rel, dir))
}

/// Add a rendered post to the review queue (status `pending`).
pub fn add_post(home: &Home, brand: &str, plan: &photocraft_press::Plan, rel: &str, files: &[String]) -> Result<Entry> {
    let mut e = Entry::new();
    e.insert("id".into(), json!(new_id()));
    e.insert("brand".into(), json!(brand));
    e.insert("title".into(), json!(plan.title()));
    e.insert("format".into(), json!(if plan.format.is_empty() { "carousel" } else { plan.format.as_str() }));
    e.insert("rel".into(), json!(rel));
    e.insert("files".into(), json!(files));
    e.insert("caption".into(), json!(plan.full_caption()));
    e.insert("status".into(), json!("pending"));
    e.insert("created".into(), json!(now_iso()));
    e.insert("origin".into(), json!("desktop"));
    let mut items = load_queue(home)?;
    items.push(e.clone());
    save_queue(home, &items)?;
    // Keep the plan next to the PNGs so the post can be rebuilt later.
    let _ = write_atomic(&home.outputs().join(rel).join("plan.json"), serde_json::to_string_pretty(&plan.to_json())?.as_bytes());
    if !plan.source_url.is_empty() {
        let _ = mark_used(home, &plan.source_url);
    }
    Ok(e)
}

/// The plan saved beside an entry's PNGs, if any.
pub fn entry_plan(home: &Home, e: &Entry) -> Option<photocraft_press::Plan> {
    let p = home.outputs().join(s(e, "rel")).join("plan.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
    photocraft_press::Plan::from_json(&v).ok()
}

/// Local paths of an entry's PNGs.
pub fn entry_files(home: &Home, e: &Entry) -> Vec<PathBuf> {
    let dir = home.outputs().join(s(e, "rel"));
    e.get("files").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(|f| dir.join(f)).collect()
}

// ── Plans ────────────────────────────────────────────────────────────────────

pub fn plans_dir(home: &Home, brand: &str) -> PathBuf {
    home.library().join("plans").join(photocraft_press::text::slug(brand, "brand"))
}

pub fn save_plan(home: &Home, brand: &str, plan: &photocraft_press::Plan) -> Result<PathBuf> {
    let dir = plans_dir(home, brand);
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{}-{stamp}.json", photocraft_press::text::slug(&plan.slug, "post")));
    write_atomic(&path, serde_json::to_string_pretty(&plan.to_json())?.as_bytes())?;
    Ok(path)
}

/// Saved plans, newest first: (path, title).
pub fn list_plans(home: &Home, brand: &str) -> Vec<(PathBuf, String)> {
    let mut v: Vec<(PathBuf, String, std::time::SystemTime)> = std::fs::read_dir(plans_dir(home, brand))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let p = e.path();
            let t = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            let title = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| photocraft_press::Plan::from_json(v.get("plan").unwrap_or(&v)).ok())
                .map(|pl| pl.title().to_string())?;
            Some((p, title, t))
        })
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.2));
    v.into_iter().take(100).map(|(p, t, _)| (p, t)).collect()
}

/// A saved plan (the web app sometimes wraps it as `{"plan": …, "image_paths": …}`).
pub fn load_plan(path: &std::path::Path) -> Result<photocraft_press::Plan> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let mut plan = photocraft_press::Plan::from_json(v.get("plan").unwrap_or(&v)).map_err(StudioError)?;
    // The web app keeps per-slide images as {"0": path, …} (0 = title, 1.. = content).
    if let Some(m) = v.get("image_paths").and_then(Value::as_object) {
        for (k, p) in m {
            let (Ok(i), Some(p)) = (k.parse::<usize>(), p.as_str()) else { continue };
            if i == 0 {
                plan.image = p.to_string();
            } else if let Some(s) = plan.content_slides.get_mut(i - 1) {
                s.image = p.to_string();
            }
        }
    }
    Ok(plan)
}

// ── Seen stories ─────────────────────────────────────────────────────────────

pub fn used_urls(home: &Home) -> BTreeSet<String> {
    std::fs::read_to_string(home.library().join("used_urls.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn mark_used(home: &Home, url: &str) -> Result<()> {
    let mut v: Vec<String> = used_urls(home).into_iter().collect();
    if v.iter().any(|u| u == url) {
        return Ok(());
    }
    v.push(url.to_string());
    std::fs::create_dir_all(home.library())?;
    write_atomic(&home.library().join("used_urls.json"), serde_json::to_string_pretty(&v)?.as_bytes())
}
