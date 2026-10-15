//! Small text helpers shared by the planner, CSV import and story de-duplication.

use std::collections::BTreeSet;

/// Kebab-case slug, at most 40 characters.
pub fn slug(text: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.is_empty() { fallback.to_string() } else { s }
}

/// Hashtags from "seo, local #marketing" style text, without `#`.
pub fn split_tags(value: &str) -> Vec<String> {
    value.split(|c: char| c == ',' || c.is_whitespace()).map(|t| t.trim().trim_start_matches('#').to_string()).filter(|t| !t.is_empty()).collect()
}

const STOPWORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "is",
    "are",
    "was",
    "were",
    "to",
    "of",
    "in",
    "on",
    "for",
    "and",
    "or",
    "with",
    "its",
    "it's",
    "this",
    "that",
    "new",
    "after",
    "before",
    "says",
    "say",
    "said",
    "how",
    "why",
    "what",
    "will",
    "has",
    "have",
    "into",
    "from",
    "by",
    "as",
    "at",
    "be",
    "been",
    "not",
    "no",
    "but",
    "so",
    "up",
    "out",
    "over",
    "under",
    "than",
    "you",
    "your",
    "now",
    "just",
    "could",
    "would",
    "should",
    "report",
    "reportedly",
    "confirms",
    "confirmed",
    "according",
    "announced",
    "announces",
    "week",
    "today",
];

fn sig_words(text: &str) -> BTreeSet<String> {
    text.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| w.len() > 2 && !STOPWORDS.contains(w)).map(str::to_string).collect()
}

/// Overlap coefficient of significant words (intersection / smaller set), as K2 Press uses to
/// spot the same event covered by different outlets.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (wa, wb) = (sig_words(a), sig_words(b));
    let smaller = wa.len().min(wb.len());
    if smaller == 0 {
        return 0.0;
    }
    wa.intersection(&wb).count() as f64 / smaller as f64
}

/// Threshold calibrated in K2 Press against real headline pairs.
pub const DEDUPE_THRESHOLD: f64 = 0.42;

/// Indices of `texts` to keep (best first in, first of each near-duplicate cluster kept).
pub fn dedupe(texts: &[String]) -> Vec<usize> {
    let mut kept: Vec<usize> = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        if !kept.iter().any(|&k| texts.get(k).is_some_and(|kt| similarity(t, kt) >= DEDUPE_THRESHOLD)) {
            kept.push(i);
        }
    }
    kept
}

/// Truncate to `max` characters at a char boundary.
pub fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Strip HTML tags and collapse whitespace (feed summaries).
pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#8217;", "’")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
