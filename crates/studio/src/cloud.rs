//! Cloud publishing through the Shorts Factory Supabase backend (the same `publishing-api` edge
//! function and the same `.env` credentials), so posts go out on schedule while the PC is off, and
//! "Post now" works without the K2 Press web app serving images. Instagram fetches each slide from
//! a short-lived signed URL the backend makes.
//!
//! A post is reserved (`createUpload` with `mediaKind` image | story | carousel and its JPEG
//! files), each slide is PUT straight to its signed storage URL (Backblaze B2 or Supabase
//! Storage), then `finalizeUploadAndSchedule` confirms it. Slides are converted from the library's
//! PNGs to JPEG first (Instagram's content publishing takes JPEG only).
//!
//! Queue entries scheduled here get the status `cloud_scheduled` and a `cloud` record, so the web
//! app's own scheduler (which sends only `scheduled` posts) never sends them a second time.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use chrono::TimeZone;
use serde_json::{Value, json};

use crate::config::Config;
use crate::home::{Env, Home};
use crate::library::{Entry, entry_files, now_iso, s};
use crate::{Result, StudioError, err};

/// Instagram's ceiling for one image.
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
struct Session {
    url: String,
    anon: String,
    access: String,
    refresh: String,
    expires_ms: i64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn session_slot() -> std::sync::MutexGuard<'static, Option<Session>> {
    static S: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(PoisonError::into_inner)
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(20)).timeout(Duration::from_secs(secs)).redirects(0).build()
}

const VARS: [&str; 4] = ["CLOUD_SUPABASE_URL", "CLOUD_SUPABASE_ANON_KEY", "CLOUD_AUTH_EMAIL", "CLOUD_AUTH_PASSWORD"];

/// Is cloud publishing set up in `.env`?
pub fn configured(env: &Env) -> bool {
    VARS.iter().all(|k| env.get(k).is_some())
}

/// The `.env` keys still missing.
pub fn missing(env: &Env) -> Vec<&'static str> {
    VARS.iter().copied().filter(|k| env.get(k).is_none()).collect()
}

fn valid_project_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|h| h.strip_suffix(".supabase.co"))
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

fn auth(url: &str, anon: &str, grant: &str, body: Value) -> Result<Session> {
    let r = agent(20)
        .post(&format!("{url}/auth/v1/token?grant_type={grant}"))
        .set("apikey", anon)
        .send_json(body)
        .map_err(|_| StudioError("cloud sign-in failed: check CLOUD_AUTH_EMAIL / CLOUD_AUTH_PASSWORD in .env and the internet connection".into()))?;
    let v: Value = r.into_json().map_err(|e| StudioError(e.to_string()))?;
    let get = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Ok(Session {
        url: url.to_string(),
        anon: anon.to_string(),
        access: get("access_token").ok_or_else(|| StudioError("cloud sign-in returned no session".into()))?,
        refresh: get("refresh_token").unwrap_or_default(),
        expires_ms: now_ms() + v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600) * 1000,
    })
}

fn active(env: &Env) -> Result<Session> {
    if !configured(env) {
        return err(format!("cloud publishing is not set up: add {} to the K2 Press .env", missing(env).join(", ")));
    }
    let url = env.get("CLOUD_SUPABASE_URL").unwrap_or_default().trim_end_matches('/').to_string();
    if !valid_project_url(&url) {
        return err("CLOUD_SUPABASE_URL must be https://PROJECT.supabase.co");
    }
    let anon = env.get("CLOUD_SUPABASE_ANON_KEY").unwrap_or_default().to_string();
    let mut slot = session_slot();
    if let Some(s) = slot.as_ref().filter(|s| s.url == url && s.anon == anon) {
        if s.expires_ms > now_ms() + 60_000 {
            return Ok(s.clone());
        }
        if let Ok(fresh) = auth(&url, &anon, "refresh_token", json!({"refresh_token": s.refresh})) {
            *slot = Some(fresh.clone());
            return Ok(fresh);
        }
    }
    let fresh = auth(&url, &anon, "password", json!({"email": env.get("CLOUD_AUTH_EMAIL"), "password": env.get("CLOUD_AUTH_PASSWORD")}))?;
    *slot = Some(fresh.clone());
    Ok(fresh)
}

/// One `publishing-api` action.
pub fn call(env: &Env, action: &str, mut body: Value) -> Result<Value> {
    let s = active(env)?;
    if !body.is_object() {
        body = json!({});
    }
    body["action"] = json!(action);
    match agent(60)
        .post(&format!("{}/functions/v1/publishing-api", s.url))
        .set("Authorization", &format!("Bearer {}", s.access))
        .set("apikey", &s.anon)
        .send_json(body)
    {
        Ok(r) => r.into_json().map_err(|e| StudioError(e.to_string())),
        Err(ureq::Error::Status(code, r)) => {
            let v: Value = r.into_json().unwrap_or(Value::Null);
            err(v.get("error").and_then(Value::as_str).map_or_else(|| format!("cloud request failed ({code})"), str::to_string))
        }
        Err(_) => err("connection required: the cloud has not confirmed this; retry the same request"),
    }
}

/// A connected cloud Instagram account.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub id: String,
    pub instagram_user_id: String,
    pub name: String,
    pub status: String,
    pub expires: String,
}

pub fn accounts(env: &Env) -> Result<Vec<Account>> {
    let v = call(env, "accounts", json!({}))?;
    let list = v.as_array().cloned().unwrap_or_default();
    Ok(list
        .iter()
        .map(|a| {
            let g = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            Account {
                id: g("id"),
                instagram_user_id: g("instagram_user_id"),
                name: g("account_name"),
                status: g("connection_status"),
                expires: g("token_expires_at"),
            }
        })
        .filter(|a| !a.id.is_empty())
        .collect())
}

/// The cloud account for a brand: the one picked in Settings, else the one whose Instagram id is
/// the brand's `meta.accounts.<brand>.ig_user_id`, else the only account.
pub fn account_for<'a>(cfg: &Config, brand: &str, picked: Option<&str>, list: &'a [Account]) -> Option<&'a Account> {
    if let Some(p) = picked.filter(|p| !p.is_empty())
        && let Some(a) = list.iter().find(|a| a.id == p)
    {
        return Some(a);
    }
    let ig = cfg.at(&format!("meta.accounts.{brand}.ig_user_id")).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).unwrap_or_default();
    let ig = ig.trim().trim_matches('"');
    if !ig.is_empty()
        && let Some(a) = list.iter().find(|a| a.instagram_user_id == ig)
    {
        return Some(a);
    }
    if list.len() == 1 { list.first() } else { None }
}

/// The slide upload destination must be this reservation's own object, on B2 or this project's
/// Storage, over HTTPS, without credentials or ports.
pub fn valid_upload_target(url: &str, project: &str, provider: &str, bucket: &str, key: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let (host, path_query) = rest.split_once('/').unwrap_or((rest, ""));
    if host.contains('@') || host.contains(':') {
        return false;
    }
    let path = format!("/{}", path_query.split('?').next().unwrap_or(""));
    match provider {
        "b2" => {
            let region_ok = host
                .strip_prefix("s3.")
                .and_then(|h| h.strip_suffix(".backblazeb2.com"))
                .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
            region_ok && path == format!("/{bucket}/{key}") && path_query.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256")
        }
        "supabase" => format!("https://{host}") == project && path == format!("/storage/v1/object/upload/sign/{bucket}/{key}"),
        _ => false,
    }
}

/// A v4-shaped UUID for idempotency keys (unique, not secret).
pub fn new_key() -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut x = (now_ms() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(std::process::id()).rotate_left(32)
        ^ N.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let (a, b) = (next(), next());
    format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}", a >> 32, (a >> 16) & 0xffff, a & 0xfff, (b >> 48) & 0x3fff | 0x8000, b & 0xffff_ffff_ffff)
}

/// What Instagram calls this post: `story`, a single `image`, or a `carousel` of 2–10.
pub fn media_kind(format: &str, slides: usize) -> Result<&'static str> {
    match (photocraft_press::Format::from_key(format), slides) {
        (_, 0) => err("the post has no slides"),
        (photocraft_press::Format::Story, 1) => Ok("story"),
        (photocraft_press::Format::Story, _) => err("a story is one image"),
        (_, 1) => Ok("image"),
        (_, 2..=10) => Ok("carousel"),
        _ => err("Instagram carousels take at most 10 images"),
    }
}

/// The slide as an Instagram-ready JPEG (RGB, at most 8 MB), written beside the PNGs.
pub fn to_jpeg(src: &Path, out: &Path) -> Result<u64> {
    let mut bytes = Vec::new();
    std::fs::File::open(src)?.take(80 * 1024 * 1024).read_to_end(&mut bytes)?;
    let img = photocraft_codecs::decode(&bytes).map_err(|e| StudioError(format!("{}: {e}", src.display())))?;
    let rgb = img.convert(photocraft_codecs::ChannelLayout::Rgb, photocraft_codecs::SampleType::U8);
    for q in [92u8, 85, 75, 60] {
        let opts = photocraft_codecs::EncodeOptions { jpeg_quality: q, ..Default::default() };
        let jpg = photocraft_codecs::encode(&rgb, photocraft_codecs::Format::Jpeg, &opts).map_err(|e| StudioError(format!("{}: {e}", src.display())))?;
        if jpg.len() as u64 <= MAX_IMAGE_BYTES {
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(out, &jpg)?;
            return Ok(jpg.len() as u64);
        }
    }
    err(format!("{} is too large for Instagram even as a compressed JPEG", src.display()))
}

fn upload(url: &str, file: &Path, size: u64) -> Result<()> {
    let f = std::fs::File::open(file)?;
    match agent(600).put(url).set("Content-Type", "image/jpeg").set("Content-Length", &size.to_string()).send(f) {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, _)) => err(format!("slide upload not confirmed ({code}); retry the same schedule")),
        Err(e) => err(format!("slide upload failed ({e}); retry the same schedule")),
    }
}

/// ISO time with the local offset, as the backend wants.
pub fn local_iso(when: chrono::NaiveDateTime) -> Result<String> {
    chrono::Local
        .from_local_datetime(&when)
        .earliest()
        .map(|t| t.to_rfc3339())
        .ok_or_else(|| StudioError("that local time does not exist (clock change)".into()))
}

fn local_timezone() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into())
}

/// Reserve, upload and schedule a queue entry in the cloud (`when` = None: publish now).
/// Stamps the outcome on `e` (the caller saves it). The idempotency key is stored on the entry
/// before anything is sent, so a retry after a timeout reuses the same reservation.
pub fn schedule_entry(home: &Home, env: &Env, e: &mut Entry, account: &Account, when: Option<chrono::NaiveDateTime>) -> Result<Value> {
    let files: Vec<PathBuf> = entry_files(home, e);
    let kind = media_kind(s(e, "format"), files.len())?;
    let scheduled_for = match when {
        Some(w) => local_iso(w)?,
        None => chrono::Local::now().to_rfc3339(),
    };
    let mut cloud = e.get("cloud").and_then(Value::as_object).cloned().unwrap_or_default();
    let prior_when = cloud.get("scheduledFor").and_then(Value::as_str).unwrap_or("").to_string();
    let prior_status = cloud.get("status").and_then(Value::as_str).unwrap_or("").to_string();
    // A new time or a finished/cancelled reservation needs a new key; a retry keeps the old one.
    let reuse = cloud.get("idempotencyKey").is_some() && prior_when == scheduled_for && matches!(prior_status.as_str(), "" | "uploading" | "reserving");
    if !reuse {
        cloud = serde_json::Map::new();
        cloud.insert("idempotencyKey".into(), json!(new_key()));
    }
    let key = cloud.get("idempotencyKey").and_then(Value::as_str).unwrap_or_default().to_string();
    cloud.insert("scheduledFor".into(), json!(scheduled_for));
    cloud.insert("account".into(), json!(account.name));
    cloud.insert("accountId".into(), json!(account.id));
    cloud.insert("status".into(), json!("reserving"));
    e.insert("cloud".into(), Value::Object(cloud.clone()));

    // JPEGs beside the PNGs (outputs/<rel>/cloud/NN.jpg).
    let dir = home.outputs().join(s(e, "rel")).join("cloud");
    let mut jpgs = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let out = dir.join(format!("{:02}.jpg", i + 1));
        let size = to_jpeg(f, &out)?;
        jpgs.push((out, size));
    }
    let list: Vec<Value> =
        jpgs.iter().map(|(p, n)| json!({"filename": p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), "fileSize": n})).collect();
    let caption: String = s(e, "caption").chars().take(2200).collect();
    let reserved = call(
        env,
        "createUpload",
        json!({
            "idempotencyKey": key, "instagramAccountId": account.id,
            "localContentId": format!("k2press:{}", s(e, "id")),
            "title": photocraft_press::text::clip(s(e, "title"), 300), "caption": caption,
            "mediaKind": kind, "mimeType": "image/jpeg", "files": list,
            "scheduledFor": scheduled_for, "timezone": local_timezone(), "publishNow": when.is_none(),
        }),
    )?;
    let post = reserved.get("post").cloned().unwrap_or(Value::Null);
    let id = post
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| StudioError("the cloud returned no reservation (is the backend updated for image posts?)".into()))?
        .to_string();
    cloud.insert("id".into(), json!(id));
    cloud.insert("status".into(), json!("uploading"));
    e.insert("cloud".into(), Value::Object(cloud.clone()));
    let urls: Vec<Option<String>> =
        reserved.get("uploadUrls").and_then(Value::as_array).map(|a| a.iter().map(|u| u.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if reserved.get("uploaded") != Some(&Value::Bool(true)) {
        if urls.len() != jpgs.len() {
            return err("the cloud backend does not take image posts yet: deploy the image-posts update (see docs/POST-STUDIO.md)");
        }
        let paths: Vec<String> =
            post.get("media_paths").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
        let provider = post.get("storage_provider").and_then(Value::as_str).unwrap_or("");
        let bucket = post.get("storage_bucket").and_then(Value::as_str).unwrap_or("");
        let project = active(env)?.url;
        for (i, ((file, size), url)) in jpgs.iter().zip(&urls).enumerate() {
            let Some(url) = url else { continue };
            let key = paths.get(i).map(String::as_str).unwrap_or("");
            if !valid_upload_target(url, &project, provider, bucket, key) {
                return err("unexpected cloud upload destination");
            }
            upload(url, file, *size)?;
        }
    }
    let done = call(env, "finalizeUploadAndSchedule", json!({"id": id}))?;
    let confirmed = done.get("cloudConfirmed") == Some(&Value::Bool(true));
    let status = done.pointer("/post/status").and_then(Value::as_str).unwrap_or("scheduled").to_string();
    cloud.insert("status".into(), json!(status));
    cloud.insert("confirmed".into(), json!(confirmed));
    cloud.insert("at".into(), json!(now_iso()));
    e.insert("cloud".into(), Value::Object(cloud));
    if confirmed {
        e.insert("status".into(), json!("cloud_scheduled"));
        if let Some(w) = when {
            e.insert("scheduled_at".into(), json!(crate::schedule::iso(w)));
        } else {
            e.insert("scheduled_at".into(), json!(crate::schedule::iso(crate::schedule::now())));
        }
        e.remove("blocked_reason");
    }
    Ok(done)
}

/// Bring queue entries scheduled in the cloud up to date (`listSchedules`). Returns how many
/// entries changed.
pub fn sync(home: &Home, env: &Env) -> Result<usize> {
    let mut items = crate::library::load_queue(home)?;
    if !items.iter().any(|e| e.get("cloud").and_then(|c| c.get("id")).is_some()) {
        return Ok(0);
    }
    let mut by_id: HashMap<String, Value> = HashMap::new();
    let mut offset = Some(0i64);
    let mut pages = 0;
    while let Some(o) = offset {
        let v = call(env, "listSchedules", json!({"offset": o}))?;
        for it in v.get("items").and_then(Value::as_array).into_iter().flatten() {
            if let Some(id) = it.pointer("/post/id").and_then(Value::as_str) {
                by_id.insert(id.to_string(), it.clone());
            }
        }
        offset = v.get("nextOffset").and_then(Value::as_i64);
        pages += 1;
        if pages >= 5 {
            break;
        }
    }
    let mut changed = 0;
    for e in &mut items {
        let Some(id) = e.get("cloud").and_then(|c| c.get("id")).and_then(Value::as_str).map(str::to_string) else { continue };
        let Some(item) = by_id.get(&id) else { continue };
        if apply_cloud_state(e, item) {
            changed += 1;
        }
    }
    if changed > 0 {
        crate::library::save_queue(home, &items)?;
    }
    Ok(changed)
}

/// Copy a cloud post's state onto its queue entry; true when something changed.
pub fn apply_cloud_state(e: &mut Entry, item: &Value) -> bool {
    let post = item.get("post").cloned().unwrap_or(Value::Null);
    let g = |k: &str| post.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let status = g("status");
    let before = Value::Object(e.clone());
    let mut cloud = e.get("cloud").and_then(Value::as_object).cloned().unwrap_or_default();
    cloud.insert("status".into(), json!(status));
    cloud.insert("stage".into(), json!(g("processing_stage")));
    if !g("last_error").is_empty() {
        cloud.insert("error".into(), json!(g("last_error")));
    }
    e.insert("cloud".into(), Value::Object(cloud));
    match status.as_str() {
        "published" => {
            e.insert("status".into(), json!("published"));
            e.insert("published_at".into(), json!(if g("published_at").is_empty() { now_iso() } else { g("published_at") }));
            e.insert(
                "publish".into(),
                json!({"sent": true, "via": "cloud", "ok": true, "results": [{"platform": "instagram", "media_id": g("instagram_media_id"), "permalink": g("instagram_permalink")}], "errors": []}),
            );
        }
        "failed" => {
            e.insert("status".into(), json!("failed"));
            e.insert("publish".into(), json!({"sent": false, "via": "cloud", "errors": [{"platform": "instagram", "error": g("last_error")}]}));
        }
        "cancelled" => {
            e.insert("status".into(), json!("approved"));
            e.remove("scheduled_at");
        }
        "scheduled" | "processing" | "publishing" => {
            e.insert("status".into(), json!("cloud_scheduled"));
        }
        _ => {}
    }
    Value::Object(e.clone()) != before
}

/// Cancel a cloud schedule (before publishing has begun).
pub fn cancel(env: &Env, cloud_id: &str) -> Result<Value> {
    call(env, "cancelPost", json!({"id": cloud_id}))
}

/// Move a scheduled cloud post to now.
pub fn publish_now(env: &Env, cloud_id: &str) -> Result<Value> {
    call(env, "publishNow", json!({"id": cloud_id}))
}
