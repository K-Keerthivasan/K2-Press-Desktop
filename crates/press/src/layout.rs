//! The post layouts. Each slide of a post compiles to a [`SlideSpec`]: a canvas and a list of
//! engine steps that draw it as ordinary layers (shape layers for panels and rules, type layers
//! for every line of copy, placed smart objects for images and the logo). The engine's
//! `press.build` runs them; the result is a normal document anyone can keep editing.
//!
//! Three looks: **Classic** (K2 Digital Media: dark panel, accent rule, logo lockup, faint photo),
//! **Image-forward** (JKR: full-bleed photo, heavy type, accent bar) and **Service** (local-business
//! clients such as Phoenix Fine Touch; see [`crate::service`]). Every look renders every platform
//! format: Instagram, Facebook, LinkedIn, X, YouTube thumbnail and banner, email header, Pinterest.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Brand, Plan};

/// Post format (platform version) and its canvas size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Instagram / Facebook carousel, 4:5 (title, content slides, outro).
    Carousel,
    /// Single 4:5 feed post (Instagram, Facebook).
    Portrait,
    /// Single 1:1 feed post.
    Square,
    /// 9:16 story / Reel cover.
    Story,
    /// X / Twitter, 16:9.
    X,
    /// Facebook link / feed image, 1.91:1.
    Facebook,
    /// LinkedIn feed image, 1.91:1.
    Linkedin,
    /// YouTube video thumbnail, 16:9.
    YoutubeThumb,
    /// YouTube channel banner (text stays in the centre 1546×423 safe area).
    YoutubeBanner,
    /// Email header / hero image (the HTML email wraps it).
    Email,
    /// Pinterest pin, 2:3.
    Pinterest,
    Quote,
    Cover,
}

impl Format {
    pub const ALL: [Format; 13] = [
        Format::Carousel,
        Format::Portrait,
        Format::Square,
        Format::Story,
        Format::X,
        Format::Facebook,
        Format::Linkedin,
        Format::YoutubeThumb,
        Format::YoutubeBanner,
        Format::Email,
        Format::Pinterest,
        Format::Quote,
        Format::Cover,
    ];

    pub fn from_key(key: &str) -> Format {
        match key.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "portrait" | "post" | "feed" => Format::Portrait,
            "square" => Format::Square,
            "story" | "reel" | "reel_cover" => Format::Story,
            "x" | "xpost" | "twitter" => Format::X,
            "facebook" | "fb" => Format::Facebook,
            "linkedin" => Format::Linkedin,
            "youtube_thumb" | "youtube" | "yt_thumb" | "thumbnail" => Format::YoutubeThumb,
            "youtube_banner" | "yt_banner" | "banner" => Format::YoutubeBanner,
            "email" | "email_header" | "newsletter" => Format::Email,
            "pinterest" | "pin" => Format::Pinterest,
            "quote" => Format::Quote,
            "cover" => Format::Cover,
            _ => Format::Carousel,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Format::Carousel => "carousel",
            Format::Portrait => "portrait",
            Format::Square => "square",
            Format::Story => "story",
            Format::X => "x",
            Format::Facebook => "facebook",
            Format::Linkedin => "linkedin",
            Format::YoutubeThumb => "youtube_thumb",
            Format::YoutubeBanner => "youtube_banner",
            Format::Email => "email",
            Format::Pinterest => "pinterest",
            Format::Quote => "quote",
            Format::Cover => "cover",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Carousel => "Instagram carousel (1080×1350)",
            Format::Portrait => "Instagram / Facebook post (1080×1350)",
            Format::Square => "Square post (1080×1080)",
            Format::Story => "Story / Reel cover (1080×1920)",
            Format::X => "X / Twitter (1600×900)",
            Format::Facebook => "Facebook image (1200×630)",
            Format::Linkedin => "LinkedIn image (1200×627)",
            Format::YoutubeThumb => "YouTube thumbnail (1280×720)",
            Format::YoutubeBanner => "YouTube channel banner (2560×1440)",
            Format::Email => "Email header (1200×600)",
            Format::Pinterest => "Pinterest pin (1000×1500)",
            Format::Quote => "Quote card (1080×1350)",
            Format::Cover => "Brand cover (1080×1350)",
        }
    }

    /// The platform a format is for, as shown in the panel.
    pub fn platform(self) -> &'static str {
        match self {
            Format::Carousel | Format::Portrait | Format::Square | Format::Story | Format::Quote | Format::Cover => "Instagram",
            Format::X => "X",
            Format::Facebook => "Facebook",
            Format::Linkedin => "LinkedIn",
            Format::YoutubeThumb | Format::YoutubeBanner => "YouTube",
            Format::Email => "Email",
            Format::Pinterest => "Pinterest",
        }
    }

    pub fn size(self) -> (u32, u32) {
        match self {
            Format::Carousel | Format::Portrait | Format::Quote | Format::Cover => (1080, 1350),
            Format::Square => (1080, 1080),
            Format::Story => (1080, 1920),
            Format::X => (1600, 900),
            Format::Facebook => (1200, 630),
            Format::Linkedin => (1200, 627),
            Format::YoutubeThumb => (1280, 720),
            Format::YoutubeBanner => (2560, 1440),
            Format::Email => (1200, 600),
            Format::Pinterest => (1000, 1500),
        }
    }

    /// Wider than tall: laid out as a text column beside the photo.
    pub fn landscape(self) -> bool {
        let (w, h) = self.size();
        w * 10 > h * 13
    }

    /// Goes to the Instagram / Facebook queue (the others are files for their platforms).
    pub fn instagram(self) -> bool {
        matches!(self, Format::Carousel | Format::Portrait | Format::Square | Format::Story | Format::Quote | Format::Cover)
    }

    /// Instagram post type when published (`carousel`, `story` or a single `post`).
    pub fn post_type(self, slides: usize) -> &'static str {
        match self {
            Format::Story => "story",
            Format::Carousel if slides > 1 => "carousel",
            _ => "post",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    Classic,
    ImageForward,
    /// Local-service client look (Phoenix Fine Touch): light photo, logo, script tagline, bold
    /// headline with a kicker band, checklist card, service icons and a contact footer bar.
    Service,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Classic, Style::ImageForward, Style::Service];

    pub fn for_brand(brand: &Brand) -> Style {
        if let Some(s) = Style::from_key(brand.style.trim()) {
            return s;
        }
        if brand.image_forward { Style::ImageForward } else { Style::Classic }
    }
    pub fn from_key(key: &str) -> Option<Style> {
        match key {
            "classic" => Some(Style::Classic),
            "imageForward" | "image_forward" | "image" => Some(Style::ImageForward),
            "service" | "client" | "local" => Some(Style::Service),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Style::Classic => "classic",
            Style::ImageForward => "imageForward",
            Style::Service => "service",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Style::Classic => "Classic",
            Style::ImageForward => "Image-forward",
            Style::Service => "Service (local business)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlideKind {
    Title,
    Content,
    Outro,
    Single,
}

impl SlideKind {
    pub fn key(self) -> &'static str {
        match self {
            SlideKind::Title => "title",
            SlideKind::Content => "content",
            SlideKind::Outro => "outro",
            SlideKind::Single => "post",
        }
    }
}

/// How a placed image fills its frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Fill the frame, cropped by a clipping mask (photos).
    Cover,
    /// Fit inside the frame, centred (logos).
    Contain,
}

/// One drawing step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum Step {
    /// Run an engine command on the slide's document.
    Cmd { id: String, params: Value },
    /// Place an image file into `rect` ([x, y, w, h]). Cover images are clipped to a frame shape
    /// (`radius` rounds its corners) so the crop stays editable.
    Image { path: String, rect: [f64; 4], fit: Fit, name: String, opacity: f64, radius: f64 },
}

/// One slide: a document to create and the steps that draw it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlideSpec {
    /// File stem when exported (`01_title`, `02_content`…), as K2 Press names its PNGs.
    pub name: String,
    pub kind: SlideKind,
    pub width: u32,
    pub height: u32,
    pub background: String,
    pub steps: Vec<Step>,
}

// ── Colour helpers ───────────────────────────────────────────────────────────

pub(crate) fn rgb(hex: &str) -> Option<[u8; 3]> {
    let h = hex.trim().trim_start_matches('#');
    let b = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 | 8 => Some([b(0)?, b(2)?, b(4)?]),
        3 => {
            let d = |i: usize| u8::from_str_radix(h.get(i..i + 1)?, 16).ok().map(|v| v * 17);
            Some([d(0)?, d(1)?, d(2)?])
        }
        _ => None,
    }
}

pub(crate) fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A valid `#rrggbb`, or `fallback`.
pub fn color(hexs: &str, fallback: &str) -> String {
    rgb(hexs).map(hex).unwrap_or_else(|| fallback.to_string())
}

/// `a` mixed toward `b` by `t` (0 = a, 1 = b).
pub fn mix(a: &str, b: &str, t: f64) -> String {
    let (Some(x), Some(y)) = (rgb(a), rgb(b)) else { return color(a, "#ffffff") };
    let t = t.clamp(0.0, 1.0);
    let m = |i: usize| (f64::from(x[i]) * (1.0 - t) + f64::from(y[i]) * t).round().clamp(0.0, 255.0) as u8;
    hex([m(0), m(1), m(2)])
}

/// `#rrggbbaa`.
pub(crate) fn alpha(c: &str, a: f64) -> String {
    format!("{}{:02x}", color(c, "#000000"), (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

// ── Text measuring (estimate; the type engine lays out the real lines) ────────

/// Lines `text` wraps to at `size` px in a `width` px box (average glyph ~0.53 em, bold ~0.58).
pub fn estimate_lines(text: &str, size: f64, width: f64, bold: bool) -> usize {
    let em = if bold { 0.58 } else { 0.52 };
    let per_line = ((width / (size * em)).floor() as usize).max(1);
    let mut lines = 0;
    for para in text.split('\n') {
        let mut cur = 0usize;
        let mut n = 1;
        for w in para.split_whitespace() {
            let l = w.chars().count();
            if cur == 0 {
                cur = l;
            } else if cur + 1 + l <= per_line {
                cur += 1 + l;
            } else {
                n += 1;
                cur = l;
            }
            while cur > per_line {
                n += 1;
                cur -= per_line;
            }
        }
        lines += n;
    }
    lines.max(1)
}

/// Largest size in `[min, max]` (steps of 2 px) at which `text` fits `max_lines` lines.
pub fn fit_size(text: &str, width: f64, max_lines: usize, min: f64, max: f64, bold: bool) -> f64 {
    let mut s = max;
    while s > min && estimate_lines(text, s, width, bold) > max_lines {
        s -= 2.0;
    }
    s.max(min)
}

pub(crate) fn height_of(text: &str, size: f64, width: f64, bold: bool, leading: f64) -> f64 {
    estimate_lines(text, size, width, bold) as f64 * size * leading
}

// ── Step builders ────────────────────────────────────────────────────────────

pub(crate) struct Ctx<'a> {
    pub(crate) brand: &'a Brand,
    pub(crate) font: String,
    pub(crate) bg: String,
    pub(crate) bg2: String,
    pub(crate) accent: String,
    pub(crate) accent2: String,
    pub(crate) fg: String,
    pub(crate) muted: String,
}

impl<'a> Ctx<'a> {
    fn new(brand: &'a Brand) -> Self {
        let t = &brand.theme;
        let bg = color(&t.navy, "#0a0f1e");
        let fg = color(&t.text, "#ffffff");
        Ctx {
            brand,
            font: brand.font().to_string(),
            bg2: color(&t.navy2, &bg),
            accent: color(&t.accent, "#00b4c8"),
            accent2: color(&t.accent2, "#00c896"),
            muted: mix(&fg, &bg, 0.38),
            bg,
            fg,
        }
    }
}

pub(crate) fn cmd(id: &str, params: Value) -> Step {
    Step::Cmd { id: id.into(), params }
}

pub(crate) fn rect(name: &str, r: [f64; 4], fill: Value) -> Step {
    cmd("shape.create", json!({"kind": "rect", "rect": r, "fill": fill, "name": name}))
}

pub(crate) fn rounded(name: &str, r: [f64; 4], radius: f64, fill: Value) -> Step {
    cmd("shape.create", json!({"kind": "roundedRect", "rect": r, "radii": radius, "fill": fill, "name": name}))
}

/// Paragraph text in a box.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_box(c: &Ctx, name: &str, text: &str, r: [f64; 4], size: f64, col: &str, bold: bool, align: &str) -> Step {
    let mut p =
        json!({"box": r, "text": text, "name": name, "font": c.font, "size": size.round(), "color": col, "align": align, "leading": (size * 1.18).round()});
    if bold {
        p["weight"] = json!(700);
    }
    cmd("type.create", p)
}

/// Point text (one line) at a baseline; `align` anchors x.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_line(c: &Ctx, name: &str, text: &str, x: f64, y: f64, size: f64, col: &str, bold: bool, align: &str) -> Step {
    let mut p = json!({"x": x, "y": y, "text": text, "name": name, "font": c.font, "size": size.round(), "color": col, "align": align});
    if bold {
        p["weight"] = json!(700);
    }
    cmd("type.create", p)
}

pub(crate) fn image(path: &str, r: [f64; 4], fit: Fit, name: &str, opacity: f64, radius: f64) -> Option<Step> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(Step::Image { path: path.into(), rect: r, fit, name: name.into(), opacity, radius })
}

/// Vertical gradient over `r`: `top` colour at the top edge, `bottom` at the bottom.
pub(crate) fn fade(name: &str, r: [f64; 4], top: &str, bottom: &str) -> Step {
    // Photoshop's angle 90 runs from the bottom (first stop) to the top (last stop).
    rect(name, r, json!({"gradient": {"stops": [[0, bottom], [1, top]], "angle": 90, "style": "linear"}}))
}

pub(crate) fn logo_lockup(c: &Ctx, steps: &mut Vec<Step>, x: f64, y: f64) {
    let has_logo = c.brand.logo.as_deref().is_some_and(|l| !l.is_empty());
    let wide = c.brand.logo_shape == "wide";
    if let Some(l) = c.brand.logo.as_deref()
        && let Some(s) = image(l, if wide { [x, y, 220.0, 72.0] } else { [x, y, 72.0, 72.0] }, Fit::Contain, "Logo", 1.0, 0.0)
    {
        steps.push(s);
    }
    if wide && has_logo {
        return;
    }
    let tx = if has_logo { x + 92.0 } else { x };
    steps.push(text_line(c, "Brand name", &c.brand.name, tx, y + 32.0, 26.0, &c.fg, true, "left"));
    if !c.brand.tagline.is_empty() {
        steps.push(text_line(c, "Tagline", &c.brand.tagline, tx, y + 62.0, 19.0, &c.muted, false, "left"));
    }
}

fn corner_logo(c: &Ctx, steps: &mut Vec<Step>, w: f64) {
    if let Some(l) = c.brand.logo.as_deref() {
        let wide = c.brand.logo_shape == "wide";
        let r = if wide { [w - 72.0 - 160.0, 64.0, 160.0, 52.0] } else { [w - 72.0 - 56.0, 60.0, 56.0, 56.0] };
        if let Some(s) = image(l, r, Fit::Contain, "Logo", 0.9, 0.0) {
            steps.push(s);
        }
    }
}

pub(crate) fn handle_footer(c: &Ctx, steps: &mut Vec<Step>, w: f64, h: f64, swipe: bool) {
    let handle = c.brand.at_handle();
    if swipe {
        if !handle.is_empty() {
            steps.push(text_line(c, "Handle", &handle, 88.0, h - 70.0, 24.0, &c.muted, false, "left"));
        }
        steps.push(text_line(c, "Swipe", "SWIPE  →", w - 88.0, h - 70.0, 22.0, &c.accent, true, "right"));
    } else if !handle.is_empty() {
        steps.push(text_line(c, "Handle", &handle, w / 2.0, h - 72.0, 24.0, &c.accent, false, "center"));
    }
}

// ── Layouts ──────────────────────────────────────────────────────────────────

pub(crate) fn spec(name: String, kind: SlideKind, fmt: Format, background: &str, steps: Vec<Step>) -> SlideSpec {
    let (width, height) = fmt.size();
    SlideSpec { name, kind, width, height, background: background.to_string(), steps }
}

pub(crate) fn first_image(plan: &Plan) -> String {
    [plan.image.as_str()].into_iter().chain(plan.content_slides.iter().map(|s| s.image.as_str())).find(|s| !s.trim().is_empty()).unwrap_or("").to_string()
}

fn title_classic(c: &Ctx, plan: &Plan, fmt: Format, img: &str) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let mut s = Vec::new();
    if let Some(i) = image(img, [0.0, 0.0, w, h], Fit::Cover, "Background photo", 0.30, 0.0) {
        s.push(i);
        s.push(fade("Fade", [0.0, 0.0, w, h], &alpha(&c.bg, 0.25), &c.bg));
    }
    logo_lockup(c, &mut s, 88.0, 84.0);
    let head = plan.title_card.headline.trim();
    let sub = plan.title_card.subhead.trim();
    let tw = w - 176.0;
    let size = fit_size(head, tw, 5, 44.0, 76.0, true);
    let hh = height_of(head, size, tw, true, 1.18);
    let sub_size = 28.0;
    let sh = if sub.is_empty() { 0.0 } else { 40.0 + height_of(sub, sub_size, tw - 80.0, false, 1.3) };
    let top = ((h - hh - sh) / 2.0).max(220.0);
    s.push(text_box(c, "Headline", head, [88.0, top, tw, hh + size], size, &c.fg, true, "left"));
    if !sub.is_empty() {
        let ry = top + hh + 18.0;
        s.push(rect("Accent rule", [88.0, ry, 120.0, 8.0], json!(c.accent)));
        s.push(text_box(c, "Subhead", sub, [88.0, ry + 30.0, tw - 80.0, sh + sub_size], sub_size, &c.muted, false, "left"));
    }
    handle_footer(c, &mut s, w, h, fmt == Format::Carousel);
    s
}

fn title_image(c: &Ctx, plan: &Plan, fmt: Format, img: &str) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let mut s = Vec::new();
    if let Some(i) = image(img, [0.0, 0.0, w, h], Fit::Cover, "Background photo", 1.0, 0.0) {
        s.push(i);
    }
    s.push(fade("Fade", [0.0, h * 0.30, w, h * 0.70], &alpha(&c.bg, 0.0), &c.bg));
    if let Some(l) = c.brand.logo.as_deref()
        && let Some(i) =
            image(l, if c.brand.logo_shape == "wide" { [72.0, 64.0, 220.0, 76.0] } else { [72.0, 64.0, 84.0, 84.0] }, Fit::Contain, "Logo", 1.0, 0.0)
    {
        s.push(i);
    }
    let head = plan.title_card.headline.trim().to_uppercase();
    let sub = plan.title_card.subhead.trim();
    let tw = w - 144.0;
    let size = fit_size(&head, tw, 4, 48.0, 84.0, true);
    let hh = height_of(&head, size, tw, true, 1.1);
    let sh = if sub.is_empty() { 0.0 } else { height_of(sub, 30.0, tw, false, 1.3) + 28.0 };
    let top = h - 150.0 - hh - sh;
    if !c.brand.category.is_empty() {
        s.push(text_line(c, "Category", &c.brand.category.to_uppercase(), 72.0, top - 46.0, 24.0, &c.accent, true, "left"));
    }
    s.push(rect("Accent bar", [72.0, top - 28.0, 140.0, 10.0], json!(c.accent)));
    s.push(text_box(c, "Headline", &head, [72.0, top, tw, hh + size], size, &c.fg, true, "left"));
    if !sub.is_empty() {
        s.push(text_box(c, "Subhead", sub, [72.0, top + hh + 22.0, tw, sh + 30.0], 30.0, &mix(&c.fg, &c.bg, 0.2), false, "left"));
    }
    handle_footer(c, &mut s, w, h, fmt == Format::Carousel);
    s
}

fn content_slide(c: &Ctx, style: Style, idx: usize, heading: &str, body: &str, img: &str, fmt: Format) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let mut s = Vec::new();
    let tw = w - 176.0;
    let has_img = !img.trim().is_empty();
    let mut y;
    match style {
        Style::ImageForward if has_img => {
            let ih = (h * 0.52).round();
            if let Some(i) = image(img, [0.0, 0.0, w, ih], Fit::Cover, "Slide photo", 1.0, 0.0) {
                s.push(i);
            }
            s.push(fade("Fade", [0.0, ih - 260.0, w, 262.0], &alpha(&c.bg2, 0.0), &c.bg2));
            s.push(text_line(c, "Number", &format!("{:02}", idx), 88.0, ih + 10.0, 88.0, &c.accent, true, "left"));
            y = ih + 50.0;
        }
        _ => {
            s.push(text_line(c, "Number", &format!("{:02}", idx), 88.0, 150.0, 40.0, &c.accent, true, "left"));
            s.push(rect("Number rule", [88.0, 172.0, 64.0, 5.0], json!(c.accent2)));
            corner_logo(c, &mut s, w);
            y = 230.0;
            if has_img {
                let ih = (h * 0.36).round();
                if let Some(i) = image(img, [88.0, y, w - 176.0, ih], Fit::Cover, "Slide photo", 1.0, 28.0) {
                    s.push(i);
                }
                y += ih + 56.0;
            }
        }
    }
    let heading = heading.trim();
    if !heading.is_empty() {
        let hs = fit_size(heading, tw, 3, 36.0, 58.0, true);
        let hh = height_of(heading, hs, tw, true, 1.18);
        s.push(text_box(c, "Heading", heading, [88.0, y, tw, hh + hs], hs, if style == Style::ImageForward { &c.fg } else { &c.accent }, true, "left"));
        y += hh + 30.0;
    }
    let body = body.trim();
    if !body.is_empty() {
        let room = (h - 140.0 - y).max(120.0);
        let mut bs: f64 = 34.0;
        while bs > 22.0 && height_of(body, bs, tw, false, 1.45) > room {
            bs -= 2.0;
        }
        s.push(text_box(c, "Body", body, [88.0, y, tw, room], bs, &mix(&c.fg, &c.bg, 0.12), false, "left"));
    }
    handle_footer(c, &mut s, w, h, true);
    s
}

fn outro_slide(c: &Ctx, plan: &Plan, fmt: Format) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let mut s = Vec::new();
    let wide = c.brand.logo_shape == "wide";
    let mut y = h * 0.26;
    if let Some(l) = c.brand.logo.as_deref() {
        let r = if wide { [w / 2.0 - 200.0, y, 400.0, 140.0] } else { [w / 2.0 - 100.0, y, 200.0, 200.0] };
        if let Some(i) = image(l, r, Fit::Contain, "Logo", 1.0, 0.0) {
            s.push(i);
            y += r[3] + 70.0;
        }
    }
    let cta = if plan.outro_card.cta.trim().is_empty() { "Follow for more" } else { plan.outro_card.cta.trim() };
    let tw = w - 176.0;
    let size = fit_size(cta, tw, 3, 40.0, 64.0, true);
    let ch = height_of(cta, size, tw, true, 1.18);
    s.push(text_box(c, "Call to action", cta, [88.0, y, tw, ch + size], size, &c.fg, true, "center"));
    y += ch + 36.0;
    s.push(rect("Accent rule", [w / 2.0 - 60.0, y, 120.0, 8.0], json!(c.accent)));
    y += 80.0;
    let handle = if plan.outro_card.handle.trim().is_empty() { c.brand.at_handle() } else { plan.outro_card.handle.trim().to_string() };
    if !handle.is_empty() {
        s.push(text_line(c, "Handle", &handle, w / 2.0, y, 38.0, &c.accent, true, "center"));
        y += 56.0;
    }
    if !c.brand.website.is_empty() {
        s.push(text_line(c, "Website", &c.brand.website, w / 2.0, y, 24.0, &c.muted, false, "center"));
    }
    if !plan.dm_keyword.trim().is_empty() {
        let pill = format!("DM “{}”", plan.dm_keyword.trim().to_uppercase());
        s.push(rounded("DM pill", [w / 2.0 - 170.0, h - 210.0, 340.0, 72.0], 36.0, json!(c.accent)));
        s.push(text_line(c, "DM keyword", &pill, w / 2.0, h - 162.0, 28.0, &c.bg, true, "center"));
    }
    s
}

fn single(c: &Ctx, style: Style, plan: &Plan, fmt: Format) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let img = first_image(plan);
    let head = [&plan.headline, &plan.title_card.headline].into_iter().find(|s| !s.trim().is_empty()).map_or("", |s| s.trim());
    let body = [&plan.body, &plan.title_card.subhead].into_iter().find(|s| !s.trim().is_empty()).map_or("", |s| s.trim());
    let mut s = Vec::new();
    match fmt {
        Format::YoutubeBanner => return yt_banner(c, plan),
        f if f.landscape() => {
            let k = h / 900.0;
            let iw = (w * 0.44).round();
            if let Some(i) = image(&img, [w - iw, 0.0, iw, h], Fit::Cover, "Photo", 1.0, 0.0) {
                s.push(i);
                s.push(rect("Accent edge", [w - iw - 10.0, 0.0, 10.0, h], json!(c.accent)));
            }
            let tw = if img.trim().is_empty() { w - 160.0 * k } else { w - iw - 160.0 * k };
            let hs = fit_size(head, tw, 4, 40.0 * k, 68.0 * k, true);
            let hh = height_of(head, hs, tw, true, 1.18);
            logo_lockup(c, &mut s, 80.0 * k, 64.0 * k);
            s.push(text_box(c, "Headline", head, [80.0 * k, 220.0 * k, tw, hh + hs], hs, &c.fg, true, "left"));
            s.push(rect("Accent rule", [80.0 * k, 240.0 * k + hh, 110.0 * k, 7.0], json!(c.accent)));
            if !body.is_empty() {
                s.push(text_box(c, "Body", body, [80.0 * k, 280.0 * k + hh, tw, (h - 380.0 * k - hh).max(40.0)], 28.0 * k, &c.muted, false, "left"));
            }
            let handle = c.brand.at_handle();
            if !handle.is_empty() {
                s.push(text_line(c, "Handle", &handle, 80.0 * k, h - 60.0 * k, 24.0 * k, &c.accent, false, "left"));
            }
        }
        Format::Story => {
            let ih = (h * 0.45).round();
            if let Some(i) = image(&img, [0.0, 0.0, w, ih], Fit::Cover, "Photo", 1.0, 0.0) {
                s.push(i);
                s.push(fade("Fade", [0.0, ih - 300.0, w, 302.0], &alpha(&c.bg, 0.0), &c.bg));
            } else {
                logo_lockup(c, &mut s, 88.0, 140.0);
            }
            let tw = w - 176.0;
            let mut y = if img.trim().is_empty() { 520.0 } else { ih + 30.0 };
            let hs = fit_size(head, tw, 5, 48.0, 84.0, true);
            let hh = height_of(head, hs, tw, true, 1.15);
            s.push(text_box(c, "Headline", head, [88.0, y, tw, hh + hs], hs, &c.fg, true, "left"));
            y += hh + 40.0;
            if !body.is_empty() {
                s.push(text_box(c, "Body", body, [88.0, y, tw, 420.0], 36.0, &c.muted, false, "left"));
            }
            let cta = if plan.cta.trim().is_empty() { "Learn more" } else { plan.cta.trim() };
            s.push(rounded("CTA pill", [w / 2.0 - 260.0, h - 330.0, 520.0, 110.0], 55.0, json!(c.accent)));
            s.push(text_line(c, "CTA", &cta.to_uppercase(), w / 2.0, h - 260.0, 38.0, &c.bg, true, "center"));
            handle_footer(c, &mut s, w, h, false);
        }
        _ => {
            if style == Style::ImageForward {
                return title_image(c, &Plan { title_card: crate::TitleCard { headline: head.into(), subhead: body.into() }, ..Plan::default() }, fmt, &img);
            }
            if let Some(i) = image(&img, [0.0, 0.0, w, h], Fit::Cover, "Background photo", 0.22, 0.0) {
                s.push(i);
            }
            logo_lockup(c, &mut s, 88.0, 80.0);
            let tw = w - 176.0;
            let hs = fit_size(head, tw, 4, 44.0, 72.0, true);
            let hh = height_of(head, hs, tw, true, 1.18);
            let mut y = 250.0;
            s.push(text_box(c, "Headline", head, [88.0, y, tw, hh + hs], hs, &c.fg, true, "left"));
            y += hh + 24.0;
            s.push(rect("Accent rule", [88.0, y, 120.0, 8.0], json!(c.accent)));
            y += 50.0;
            if !body.is_empty() {
                s.push(text_box(c, "Body", body, [88.0, y, tw, h - y - 140.0], 32.0, &mix(&c.fg, &c.bg, 0.15), false, "left"));
            }
            handle_footer(c, &mut s, w, h, false);
        }
    }
    s
}

/// YouTube channel banner: everything inside the 1546×423 safe area at the centre.
fn yt_banner(c: &Ctx, plan: &Plan) -> Vec<Step> {
    let (w, h) = (2560.0, 1440.0);
    let (sx, sy, sw, sh) = ((w - 1546.0) / 2.0, (h - 423.0) / 2.0, 1546.0, 423.0);
    let mut s = Vec::new();
    let img = first_image(plan);
    if let Some(i) = image(&img, [0.0, 0.0, w, h], Fit::Cover, "Background photo", 0.35, 0.0) {
        s.push(i);
    }
    s.push(rect("Glow", [0.0, 0.0, w, h], json!({"gradient": {"stops": [[0, alpha(&c.accent, 0.30)], [1, alpha(&c.bg, 0.0)]], "angle": 0, "style": "radial", "scale": 90}})));
    if let Some(l) = c.brand.logo.as_deref()
        && let Some(i) = image(l, [sx, sy + 40.0, 380.0, sh - 80.0], Fit::Contain, "Logo", 1.0, 0.0)
    {
        s.push(i);
    }
    let head = [&plan.headline, &plan.title_card.headline].into_iter().find(|t| !t.trim().is_empty()).map_or(c.brand.tagline.as_str(), |t| t.trim());
    let tx = sx + 440.0;
    let tw = sw - 440.0;
    let hs = fit_size(head, tw, 2, 48.0, 96.0, true);
    s.push(text_box(c, "Headline", head, [tx, sy + 70.0, tw, 240.0], hs, &c.fg, true, "left"));
    s.push(rect("Accent rule", [tx, sy + 300.0, 160.0, 10.0], json!(c.accent)));
    let handle = c.brand.at_handle();
    let line = [handle.as_str(), c.brand.website.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join("   ·   ");
    if !line.is_empty() {
        s.push(text_line(c, "Handle", &line, tx, sy + 380.0, 34.0, &c.accent, true, "left"));
    }
    s
}

fn quote_card(c: &Ctx, plan: &Plan, fmt: Format) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let quote = [&plan.quote, &plan.headline, &plan.title_card.headline].into_iter().find(|s| !s.trim().is_empty()).map_or("", |s| s.trim());
    let who = [&plan.subhead, &plan.title_card.subhead].into_iter().find(|s| !s.trim().is_empty()).map_or("", |s| s.trim());
    let mut s = Vec::new();
    s.push(text_line(c, "Quote mark", "“", 88.0, 360.0, 260.0, &c.accent, true, "left"));
    let tw = w - 176.0;
    let qs = fit_size(quote, tw, 7, 38.0, 64.0, true);
    let qh = height_of(quote, qs, tw, true, 1.22);
    s.push(text_box(c, "Quote", quote, [88.0, 400.0, tw, qh + qs], qs, &c.fg, true, "left"));
    if !who.is_empty() {
        s.push(rect("Accent rule", [88.0, 440.0 + qh, 90.0, 6.0], json!(c.accent)));
        s.push(text_line(c, "Attribution", &format!("— {who}"), 88.0, 510.0 + qh, 30.0, &c.muted, false, "left"));
    }
    corner_logo(c, &mut s, w);
    handle_footer(c, &mut s, w, h, false);
    s
}

fn cover_card(c: &Ctx, fmt: Format) -> Vec<Step> {
    let (w, h) = fmt.size();
    let (w, h) = (f64::from(w), f64::from(h));
    let mut s = vec![rect(
        "Glow",
        [0.0, 0.0, w, h],
        json!({"gradient": {"stops": [[0, alpha(&c.accent, 0.35)], [1, alpha(&c.bg, 0.0)]], "angle": 90, "style": "radial", "scale": 120}}),
    )];
    let mut y = h * 0.24;
    if let Some(l) = c.brand.logo.as_deref()
        && let Some(i) = image(l, [w / 2.0 - 170.0, y, 340.0, 340.0], Fit::Contain, "Logo", 1.0, 0.0)
    {
        s.push(i);
        y += 400.0;
    } else {
        s.push(text_line(
            c,
            "Wordmark",
            if c.brand.short.is_empty() { &c.brand.name } else { &c.brand.short },
            w / 2.0,
            y + 260.0,
            260.0,
            &c.fg,
            true,
            "center",
        ));
        y += 340.0;
    }
    s.push(text_line(c, "Brand name", &c.brand.name, w / 2.0, y, 58.0, &c.fg, true, "center"));
    y += 64.0;
    if !c.brand.tagline.is_empty() {
        s.push(text_line(c, "Tagline", &c.brand.tagline, w / 2.0, y, 30.0, &c.muted, false, "center"));
        y += 70.0;
    }
    s.push(rect("Accent rule", [w / 2.0 - 70.0, y, 140.0, 8.0], json!(c.accent)));
    if !c.brand.services.is_empty() {
        s.push(text_line(c, "Services", &c.brand.services, w / 2.0, y + 80.0, 28.0, &c.accent2, true, "center"));
    }
    handle_footer(c, &mut s, w, h, false);
    s
}

/// Every slide of `plan` for `brand`, in order. `style` defaults to the brand's look.
pub fn slides(plan: &Plan, brand: &Brand, format: Option<Format>, style: Option<Style>) -> Vec<SlideSpec> {
    let fmt = format.unwrap_or_else(|| plan.format());
    let style = style.unwrap_or_else(|| Style::for_brand(brand));
    let c = Ctx::new(brand);
    if style == Style::Service {
        return crate::service::slides(&c, plan, fmt);
    }
    match fmt {
        Format::Carousel => {
            let img = if plan.image.trim().is_empty() { plan.content_slides.first().map(|s| s.image.clone()).unwrap_or_default() } else { plan.image.clone() };
            let title = match style {
                Style::ImageForward => title_image(&c, plan, fmt, &img),
                _ => title_classic(&c, plan, fmt, &img),
            };
            let mut out = vec![spec("01_title".into(), SlideKind::Title, fmt, &c.bg, title)];
            for (i, cs) in plan.content_slides.iter().take(8).enumerate() {
                let n = i + 2;
                out.push(spec(
                    format!("{n:02}_content"),
                    SlideKind::Content,
                    fmt,
                    &c.bg2,
                    content_slide(&c, style, i + 1, &cs.heading, &cs.body, &cs.image, fmt),
                ));
            }
            let n = out.len() + 1;
            out.push(spec(format!("{n:02}_outro"), SlideKind::Outro, fmt, &c.bg, outro_slide(&c, plan, fmt)));
            out
        }
        Format::Quote => vec![spec("01_quote".into(), SlideKind::Single, fmt, &c.bg, quote_card(&c, plan, fmt))],
        Format::Cover => vec![spec("01_cover".into(), SlideKind::Single, fmt, &c.bg, cover_card(&c, fmt))],
        _ => vec![spec(format!("01_{}", fmt.key()), SlideKind::Single, fmt, &c.bg, single(&c, style, plan, fmt))],
    }
}
