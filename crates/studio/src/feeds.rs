//! RSS 2.0 / Atom stories (incl. YouTube channel feeds), as K2 Press's `feeds.py`.

use std::time::Duration;

use photocraft_press::text::{clip, strip_html};

use crate::{Result, Story, StudioError};

const UA: &str = "K2PressDesktop/1.0 (+feed reader)";
/// Feed documents larger than this are refused.
const MAX_FEED_BYTES: u64 = 8 * 1024 * 1024;

pub fn fetch(url: &str) -> Result<Vec<Story>> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent(UA).build();
    let resp = agent.get(url).call().map_err(|e| StudioError(format!("{url}: {}", crate::llm::http_error(e))))?;
    let mut text = String::new();
    use std::io::Read;
    resp.into_reader().take(MAX_FEED_BYTES).read_to_string(&mut text).map_err(|e| StudioError(format!("{url}: {e}")))?;
    parse(&text, url)
}

fn child_text<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name).and_then(|c| c.text())
}

fn source_name(url: &str) -> String {
    url.split("//").nth(1).unwrap_or(url).split('/').next().unwrap_or("").trim_start_matches("www.").to_string()
}

fn first_img_src(html: &str) -> Option<String> {
    let i = html.find("<img")?;
    let rest = html.get(i..)?;
    let s = rest.find("src=")? + 4;
    let q = rest.get(s..s + 1)?;
    let rest = rest.get(s + 1..)?;
    let e = rest.find(q)?;
    rest.get(..e).map(str::to_string)
}

/// Parse a feed document.
pub fn parse(text: &str, url: &str) -> Result<Vec<Story>> {
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(text, opts).map_err(|e| StudioError(format!("{url}: not a feed ({e})")))?;
    let src = source_name(url);
    let mut out = Vec::new();
    for n in doc.descendants().filter(|n| n.is_element() && matches!(n.tag_name().name(), "item" | "entry")) {
        let title = strip_html(child_text(n, "title").unwrap_or(""));
        if title.is_empty() {
            continue;
        }
        let link = n
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "link")
            .find_map(|c| c.attribute("href").filter(|_| c.attribute("rel").is_none_or(|r| r == "alternate")).map(str::to_string).or_else(|| c.text().map(|t| t.trim().to_string())))
            .unwrap_or_default();
        let raw_summary = ["description", "summary", "content", "encoded"].iter().find_map(|k| child_text(n, k)).unwrap_or("");
        let mut summary = strip_html(raw_summary);
        // YouTube: the description lives in media:group/media:description.
        if summary.is_empty() {
            summary = n.descendants().find(|d| d.is_element() && d.tag_name().name() == "description").and_then(|d| d.text()).map(strip_html).unwrap_or_default();
        }
        let published = ["pubDate", "published", "updated", "date"].iter().find_map(|k| child_text(n, k)).unwrap_or("").trim().to_string();
        let image = n
            .descendants()
            .filter(|d| d.is_element())
            .find_map(|d| match d.tag_name().name() {
                "thumbnail" | "content" if d.attribute("url").is_some() => d.attribute("url").map(str::to_string),
                "enclosure" if d.attribute("type").is_some_and(|t| t.starts_with("image")) => d.attribute("url").map(str::to_string),
                _ => None,
            })
            .or_else(|| first_img_src(raw_summary))
            .unwrap_or_default();
        out.push(Story { title, summary: clip(&summary, 2000), url: link, published, image, source: src.clone(), score: None, reason: String::new() });
        if out.len() >= 200 {
            break;
        }
    }
    Ok(out)
}

/// All stories from `urls`, newest feeds' order kept, stories already turned into posts dropped.
/// Feeds that fail are reported, not fatal.
pub fn fetch_all(urls: &[String], seen: &std::collections::BTreeSet<String>) -> (Vec<Story>, Vec<String>) {
    let mut stories = Vec::new();
    let mut errors = Vec::new();
    for u in urls {
        match fetch(u) {
            Ok(s) => stories.extend(s.into_iter().filter(|s| s.url.is_empty() || !seen.contains(&s.url))),
            Err(e) => errors.push(e.0),
        }
    }
    let mut urls_seen = std::collections::BTreeSet::new();
    stories.retain(|s| s.url.is_empty() || urls_seen.insert(s.url.clone()));
    (stories, errors)
}
