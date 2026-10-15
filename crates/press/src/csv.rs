//! Spreadsheet → posts, as K2 Press's CSV Import: loose column matching, wide (row per post,
//! `slideN_heading` columns) and long (row per slide, grouped by `post_id`) sheets, and `to_plan`
//! for the direct mode (the sheet's copy used verbatim, no AI).

use std::collections::BTreeMap;

use crate::text::{slug, split_tags};
use crate::{Brand, ContentSlide, OutroCard, Plan, TitleCard};

const ALIASES: &[(&str, &[&str])] = &[
    ("title", &["title", "headline", "posttitle", "hook", "topic", "name"]),
    ("subtitle", &["subtitle", "subhead", "subheadline", "tagline", "hooksub"]),
    ("cta", &["cta", "calltoaction", "outro", "closing", "endcard"]),
    ("caption", &["caption", "postcaption", "description", "copy", "igcaption"]),
    ("hashtags", &["hashtags", "tags", "hashtag"]),
    ("image_query", &["imagequery", "image", "imagesearch", "photo", "visual", "imageprompt"]),
    ("notes", &["notes", "brief", "summary", "details", "source", "idea"]),
    ("format", &["format", "type", "posttype", "template"]),
    ("brand", &["brand", "account", "page"]),
    ("tone", &["tone", "voice", "stance"]),
    ("schedule", &["schedule", "scheduledat", "date", "publishat", "when", "datetime"]),
    ("post_id", &["postid", "group", "groupid", "post", "id", "slug"]),
    ("order", &["order", "slide", "slideno", "slidenumber", "seq", "sequence", "position"]),
    ("heading", &["heading", "slideheading", "slidetitle", "header"]),
    ("body", &["body", "slidebody", "text", "slidetext", "points", "bullets", "content"]),
];

/// The sample sheet offered by "Save template".
pub const TEMPLATE_CSV: &str = "title,subtitle,slide1_heading,slide1_body,slide2_heading,slide2_body,slide3_heading,slide3_body,cta,caption,hashtags,image_query\n\
\"5 SEO myths that cost you leads\",\"What actually moves local rankings\",\"KEYWORD STUFFING\",\"— Hurts readability\n— Search engines ignore it\",\"BACKLINKS ONLY\",\"— Relevance beats volume\n— Content still wins\",\"IGNORING SPEED\",\"— Slow pages lose bookings\n— Fix images first\",\"Save this for your next audit\",\"The full breakdown is in the carousel.\",\"seo,localbusiness,marketing\",\"laptop desk office\"\n";

/// One post read from a sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CsvPost {
    pub title: String,
    pub subtitle: String,
    pub cta: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub image_query: String,
    pub notes: String,
    pub format: String,
    pub brand: String,
    pub tone: String,
    pub schedule: String,
    pub slug: String,
    pub slides: Vec<ContentSlide>,
}

fn norm(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

fn field_for(header: &str) -> Option<&'static str> {
    let n = norm(header);
    ALIASES.iter().find(|(_, names)| names.contains(&n.as_str())).map(|(f, _)| *f)
}

/// `slide1_heading` / `slide_2_body` / `s3heading` → (index, part).
fn slide_col(header: &str) -> Option<(u32, &'static str)> {
    let n = norm(header);
    let rest = ["slide", "card", "s"].iter().find_map(|p| n.strip_prefix(p))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let idx: u32 = digits.parse().ok()?;
    let part = match rest.get(digits.len()..)? {
        "" | "heading" | "title" => "heading",
        "body" | "text" => "body",
        "image" | "imagequery" => "image_query",
        _ => return None,
    };
    Some((idx, part))
}

/// Parse CSV text (RFC 4180 quoting; `,`, `;` or tab, sniffed from the header line).
/// A sheet's header row and its rows keyed by header.
pub type Sheet = (Vec<String>, Vec<BTreeMap<String, String>>);

pub fn parse(text: &str) -> Result<Sheet, String> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let delim = [',', ';', '\t'].into_iter().max_by_key(|d| first.matches(*d).count()).unwrap_or(',');
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut rec: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == delim {
            rec.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            rec.push(std::mem::take(&mut field));
            if rec.iter().any(|f| !f.trim().is_empty()) {
                records.push(std::mem::take(&mut rec));
            } else {
                rec.clear();
            }
        } else {
            field.push(c);
        }
        if records.len() > 10_000 {
            return Err("the sheet has more than 10,000 rows".into());
        }
    }
    if quoted {
        return Err("a quoted field is never closed".into());
    }
    rec.push(field);
    if rec.iter().any(|f| !f.trim().is_empty()) {
        records.push(rec);
    }
    let mut it = records.into_iter();
    let headers: Vec<String> = it.next().ok_or("the sheet is empty")?.into_iter().map(|h| h.trim().to_string()).collect();
    if headers.iter().all(String::is_empty) {
        return Err("the sheet has no header row".into());
    }
    let rows = it.map(|r| headers.iter().enumerate().map(|(i, h)| (h.clone(), r.get(i).map(|v| v.trim().to_string()).unwrap_or_default())).collect()).collect();
    Ok((headers, rows))
}

/// `long` when the sheet has one row per slide, else `wide`.
pub fn detect_shape(headers: &[String]) -> &'static str {
    let fields: Vec<_> = headers.iter().filter_map(|h| field_for(h)).collect();
    if fields.contains(&"post_id") && (fields.contains(&"heading") || fields.contains(&"body")) { "long" } else { "wide" }
}

fn mapping(headers: &[String]) -> BTreeMap<&'static str, String> {
    let mut m = BTreeMap::new();
    for h in headers {
        if slide_col(h).is_some() {
            continue;
        }
        if let Some(f) = field_for(h) {
            m.entry(f).or_insert_with(|| h.clone());
        }
    }
    m
}

fn get(row: &BTreeMap<String, String>, m: &BTreeMap<&'static str, String>, field: &str) -> String {
    m.get(field).and_then(|c| row.get(c)).cloned().unwrap_or_default()
}

/// Normalise a parsed sheet into posts.
pub fn to_posts(headers: &[String], rows: &[BTreeMap<String, String>]) -> Vec<CsvPost> {
    let m = mapping(headers);
    let mut posts = Vec::new();
    if detect_shape(headers) == "long" {
        let mut order: Vec<String> = Vec::new();
        let mut groups: BTreeMap<String, Vec<&BTreeMap<String, String>>> = BTreeMap::new();
        for row in rows {
            let mut gid = get(row, &m, "post_id");
            if gid.is_empty() {
                gid = get(row, &m, "title");
            }
            if gid.is_empty() {
                gid = "post".into();
            }
            if !groups.contains_key(&gid) {
                order.push(gid.clone());
            }
            groups.entry(gid).or_default().push(row);
        }
        for gid in order {
            let Some(mut grp) = groups.remove(&gid) else { continue };
            grp.sort_by_key(|r| get(r, &m, "order").trim().parse::<f64>().map_or(i64::MAX, |v| v as i64));
            let Some(head) = grp.first().copied() else { continue };
            let slides = grp
                .iter()
                .filter(|r| !get(r, &m, "heading").is_empty() || !get(r, &m, "body").is_empty())
                .map(|r| ContentSlide {
                    heading: get(r, &m, "heading"),
                    body: get(r, &m, "body"),
                    image_query: get(r, &m, "image_query"),
                    image: String::new(),
                })
                .collect();
            posts.push(finish(head, &m, slides, &gid));
        }
    } else {
        for row in rows {
            let mut buckets: BTreeMap<u32, ContentSlide> = BTreeMap::new();
            for h in headers {
                let Some((idx, part)) = slide_col(h) else { continue };
                let v = row.get(h).map(|s| s.trim().to_string()).unwrap_or_default();
                if v.is_empty() {
                    continue;
                }
                let s = buckets.entry(idx).or_default();
                match part {
                    "heading" => s.heading = v,
                    "body" => s.body = v,
                    _ => s.image_query = v,
                }
            }
            posts.push(finish(row, &m, buckets.into_values().collect(), ""));
        }
    }
    posts.into_iter().filter(|p| !p.title.is_empty() || !p.slides.is_empty() || !p.notes.is_empty()).collect()
}

fn finish(row: &BTreeMap<String, String>, m: &BTreeMap<&'static str, String>, slides: Vec<ContentSlide>, gid: &str) -> CsvPost {
    let title = Some(get(row, m, "title")).filter(|t| !t.is_empty()).unwrap_or_else(|| gid.to_string());
    let mut notes = get(row, m, "notes");
    if notes.is_empty() && slides.is_empty() {
        notes = get(row, m, "body");
    }
    CsvPost {
        slug: slug(&title, &slug(gid, "post")),
        title,
        subtitle: get(row, m, "subtitle"),
        cta: get(row, m, "cta"),
        caption: get(row, m, "caption"),
        hashtags: split_tags(&get(row, m, "hashtags")),
        image_query: get(row, m, "image_query"),
        notes,
        format: get(row, m, "format").to_lowercase(),
        brand: get(row, m, "brand"),
        tone: get(row, m, "tone"),
        schedule: get(row, m, "schedule"),
        slides,
    }
}

/// Direct mode: the sheet's copy becomes the plan verbatim.
pub fn to_plan(post: &CsvPost, brand: &Brand, format: &str) -> Plan {
    let mut slides = post.slides.clone();
    if slides.is_empty() {
        slides.push(ContentSlide { body: post.notes.clone(), image_query: post.image_query.clone(), ..Default::default() });
    }
    for s in &mut slides {
        s.heading = s.heading.to_uppercase();
        if s.image_query.is_empty() {
            s.image_query = post.image_query.clone();
        }
    }
    let fmt = if format.is_empty() { if post.format.is_empty() { "carousel" } else { post.format.as_str() } } else { format };
    let hashtags = if post.hashtags.is_empty() { brand.hashtags.clone() } else { post.hashtags.clone() };
    let cta = if post.cta.is_empty() { "Follow for more".to_string() } else { post.cta.clone() };
    let caption = if post.caption.is_empty() { post.title.clone() } else { post.caption.clone() };
    let mut plan = Plan {
        format: fmt.into(),
        slug: post.slug.clone(),
        caption,
        hashtags,
        image_query: if post.image_query.is_empty() { slides.first().map(|s| s.image_query.clone()).unwrap_or_default() } else { post.image_query.clone() },
        platform: "instagram".into(),
        content_type: "general".into(),
        origin: "csv".into(),
        tone: post.tone.clone(),
        ..Default::default()
    };
    if matches!(fmt, "carousel" | "listicle") {
        plan.title_card = TitleCard {
            headline: post.title.clone(),
            subhead: if post.subtitle.is_empty() {
                crate::text::clip(&slides.first().map(|s| s.body.clone()).unwrap_or_default(), 90)
            } else {
                post.subtitle.clone()
            },
        };
        plan.content_slides = slides.into_iter().take(8).collect();
        plan.outro_card = OutroCard { cta, handle: brand.at_handle() };
    } else {
        plan.headline = post.title.clone();
        plan.subhead = post.subtitle.clone();
        plan.body = slides.first().map(|s| s.body.clone()).unwrap_or_default();
        plan.cta = cta;
    }
    plan
}

/// AI mode: the row as a brief (title + notes) for the planner.
pub fn to_brief(post: &CsvPost) -> (String, String) {
    let mut notes = post.notes.clone();
    if notes.is_empty() {
        let parts: Vec<String> = std::iter::once(post.subtitle.clone())
            .chain(post.slides.iter().map(|s| format!("{}: {}", s.heading, s.body).trim_matches(|c| c == ':' || c == ' ').to_string()))
            .filter(|p| !p.is_empty())
            .collect();
        notes = parts.join("\n");
    }
    if notes.is_empty() {
        notes = post.title.clone();
    }
    (post.title.clone(), notes)
}
