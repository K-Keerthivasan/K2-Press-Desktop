//! Post Studio services for K2 Press Desktop: everything the earlier K2 Press web app did besides
//! drawing, ported to Rust and reading the same folder in place:
//!
//! - [`home`]: the K2 Press folder (`config.yaml`, `.env`, `library/`, `outputs/`, `image_cache/`)
//!   and this app's own settings.
//! - [`config`]: brands, posting slots, Meta accounts and the LLM settings from `config.yaml`.
//! - [`llm`] / [`planner`]: Hermes CLI or any OpenAI-compatible server (Ollama) writing post plans,
//!   scoring stories and polishing copy in the brand's voice.
//! - [`feeds`]: RSS / Atom stories per brand category, de-duplicated.
//! - [`images`]: Pexels / Unsplash search and image download into `image_cache/`.
//! - [`library`]: the review queue / calendar (`library/review_queue.json`), saved plans and seen
//!   stories, shared with the web app.
//! - [`schedule`]: posting slots, due checks and the month grid.
//! - [`meta`]: Instagram and Facebook publishing through the Graph API.
//! - [`cloud`]: scheduling and posting through the Shorts Factory Supabase backend (works while
//!   the PC is off, and needs no public image URL).
//!
//! Network and disk calls block: callers run them off the UI thread.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
pub mod cloud;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod feeds;
#[cfg(not(target_arch = "wasm32"))]
pub mod home;
#[cfg(not(target_arch = "wasm32"))]
pub mod images;
#[cfg(not(target_arch = "wasm32"))]
pub mod library;
#[cfg(not(target_arch = "wasm32"))]
pub mod llm;
#[cfg(not(target_arch = "wasm32"))]
pub mod meta;
#[cfg(not(target_arch = "wasm32"))]
pub mod planner;
#[cfg(not(target_arch = "wasm32"))]
pub mod schedule;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

/// Errors carry a message a person can act on.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioError(pub String);

impl std::fmt::Display for StudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StudioError {}

impl From<std::io::Error> for StudioError {
    fn from(e: std::io::Error) -> Self {
        StudioError(e.to_string())
    }
}

impl From<serde_json::Error> for StudioError {
    fn from(e: serde_json::Error) -> Self {
        StudioError(format!("bad JSON: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, StudioError>;

/// The date/time crate the calendar uses, for callers that show it.
#[cfg(not(target_arch = "wasm32"))]
pub use chrono;

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(StudioError(msg.into()))
}

/// A story from a feed (or a manual brief).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Story {
    pub title: String,
    pub summary: String,
    pub url: String,
    pub published: String,
    pub image: String,
    pub source: String,
    /// AI relevance score 0–100 (`None` until scored).
    pub score: Option<i64>,
    pub reason: String,
}
