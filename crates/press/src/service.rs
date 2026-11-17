//! The **Service** look, for local-business clients (first: Phoenix Fine Touch, cleaning services
//! in London, Ontario). Modelled on their posts: a bright room photo washed light on the text side,
//! the wide logo top-left, a handwritten tagline top-right, a kicker band, a heavy headline in the
//! brand's deep colour with a gold underline, a checklist card, the services row (icon + label) and
//! a contact footer bar (location · phone · website).
//!
//! Sizes scale with the canvas (`u` = shorter side / 1080), so one recipe covers every platform:
//! tall and square posts stack, wide formats (Facebook, LinkedIn, X, YouTube thumbnail, email
//! header) put the text column beside the photo, and the YouTube banner keeps everything in its
//! centre safe area.

use serde_json::json;

use crate::layout::{Ctx, Fit, Format, SlideKind, SlideSpec, Step, alpha, cmd, first_image, fit_size, height_of, image, rect, rounded, spec, text_box, text_line};
use crate::{ContentSlide, Plan};

const WHITE: &str = "#ffffff";

/// Bullet lines from "— a\n— b" (dashes, dots and stars stripped).
pub fn bullets(body: &str) -> Vec<String> {
    body.lines().map(|l| l.trim().trim_start_matches(['—', '–', '-', '•', '*', '✓']).trim().to_string()).filter(|l| !l.is_empty()).take(6).collect()
}

/// Text in the brand's script face.
fn script(c: &Ctx, name: &str, text: &str, r: [f64; 4], size: f64, col: &str, align: &str) -> Step {
    cmd("type.create", json!({"box": r, "text": text, "name": name, "font": c.brand.script_font(), "size": size.round(), "color": col, "align": align, "leading": (size * 1.15).round()}))
}

/// Heavy display text (the brand font, bold).
fn display(c: &Ctx, name: &str, text: &str, r: [f64; 4], size: f64, col: &str, align: &str) -> Step {
    cmd("type.create", json!({"box": r, "text": text, "name": name, "font": c.font, "size": size.round(), "color": col, "align": align, "weight": 900, "leading": (size * 1.05).round()}))
}

/// A tick inside a rounded square.
fn check_box(steps: &mut Vec<Step>, x: f64, y: f64, s: f64, col: &str) {
    steps.push(rounded("Check box", [x, y, s, s], s * 0.22, json!(col)));
    let k = |fx: f64, fy: f64| [x + s * fx, y + s * fy];
    steps.push(cmd(
        "shape.create",
        json!({"kind": "path", "name": "Tick", "fill": null,
               "stroke": {"width": (s * 0.13).max(2.0), "color": WHITE, "cap": "round", "join": "round"},
               "path": {"subpaths": [{"closed": false, "knots": [k(0.24, 0.52), k(0.43, 0.70), k(0.77, 0.32)]}]}}),
    ));
}

struct Frame {
    w: f64,
    h: f64,
    u: f64,
    m: f64,
}

impl Frame {
    fn of(fmt: Format) -> Frame {
        let (w, h) = fmt.size();
        let (w, h) = (f64::from(w), f64::from(h));
        let u = w.min(h) / 1080.0;
        Frame { w, h, u, m: 64.0 * u }
    }
}

/// Photo (or a soft glow) and the light wash that keeps text readable.
fn background(c: &Ctx, s: &mut Vec<Step>, f: &Frame, img: &str, wide: bool) {
    let has = !img.trim().is_empty();
    if has {
        let r = if wide { [f.w * 0.42, 0.0, f.w * 0.58, f.h] } else { [0.0, 0.0, f.w, f.h] };
        if let Some(i) = image(img, r, Fit::Cover, "Room photo", 1.0, 0.0) {
            s.push(i);
        }
        // White on the text side, fading to the photo (angle 0 runs left to right).
        let stops = if wide { json!([[0, alpha(WHITE, 1.0)], [0.46, alpha(WHITE, 0.97)], [0.62, alpha(WHITE, 0.55)], [1, alpha(WHITE, 0.0)]]) } else { json!([[0, alpha(WHITE, 0.94)], [0.55, alpha(WHITE, 0.78)], [1, alpha(WHITE, 0.25)]]) };
        s.push(rect("Light wash", [0.0, 0.0, f.w, f.h], json!({"gradient": {"stops": stops, "angle": 0, "style": "linear"}})));
    } else {
        s.push(rect("Glow", [0.0, 0.0, f.w, f.h], json!({"gradient": {"stops": [[0, alpha(&c.accent2, 0.22)], [1, alpha(WHITE, 0.0)]], "angle": 45, "style": "radial", "scale": 140}})));
    }
}

fn logo(c: &Ctx, s: &mut Vec<Step>, r: [f64; 4]) {
    match c.brand.logo.as_deref().and_then(|l| image(l, r, Fit::Contain, "Logo", 1.0, 0.0)) {
        Some(i) => s.push(i),
        None => s.push(display(c, "Brand name", &c.brand.name, r, (r[3] * 0.38).max(18.0), &c.accent, "left")),
    }
}

/// A rounded band with centred text; returns its height.
fn band(c: &Ctx, s: &mut Vec<Step>, name: &str, text: &str, x: f64, y: f64, size: f64, fill: &str, col: &str, max_w: f64) -> f64 {
    let text = text.trim().to_uppercase();
    if text.is_empty() {
        return 0.0;
    }
    let bw = ((text.chars().count() as f64) * size * 0.66 + size * 1.6).min(max_w);
    let bh = size * 1.7;
    s.push(rounded(&format!("{name} band"), [x, y, bw, bh], bh * 0.22, json!(fill)));
    s.push(text_line(c, name, &text, x + bw / 2.0, y + bh * 0.68, size, col, true, "center"));
    bh
}

/// The headline plus its gold underline; returns the height used.
fn headline(c: &Ctx, s: &mut Vec<Step>, text: &str, x: f64, y: f64, w: f64, max_lines: usize, min: f64, max: f64) -> f64 {
    let text = text.trim().to_uppercase();
    if text.is_empty() {
        return 0.0;
    }
    let wm = w * 0.82;
    let size = fit_size(&text, wm, max_lines, min, max, true);
    let hh = height_of(&text, size, wm, true, 1.08);
    s.push(display(c, "Headline", &text, [x, y, w, hh + size], size, &c.accent, "left"));
    let uy = y + hh + size * 0.15;
    s.push(rounded("Gold underline", [x, uy, (w * 0.38).min(size * 5.0), (size * 0.13).max(6.0)], size * 0.07, json!(c.accent2)));
    hh + size * 0.45
}

/// The checklist card; returns its height.
fn checklist(c: &Ctx, s: &mut Vec<Step>, title: &str, items: &[String], x: f64, y: f64, w: f64, u: f64) -> f64 {
    if items.is_empty() {
        return 0.0;
    }
    let pad = 34.0 * u;
    let size = 34.0 * u;
    let line = size * 1.22;
    let text_w = w - pad * 2.0 - size * 1.5;
    let mut rows = 0.0;
    for it in items {
        rows += crate::layout::estimate_lines(it, size, text_w, false) as f64 * line + size * 0.55;
    }
    let th = if title.is_empty() { 0.0 } else { size * 1.7 };
    let h = pad * 2.0 + th + rows;
    s.push(rounded("Checklist card", [x, y, w, h], 26.0 * u, json!(WHITE)));
    s.push(rect("Card edge", [x, y + h - 6.0 * u, w, 6.0 * u], json!(alpha(&c.accent, 0.10))));
    let mut cy = y + pad;
    if !title.is_empty() {
        s.push(display(c, "Card title", &title.to_uppercase(), [x + pad, cy, w - pad * 2.0, size * 1.6], size * 1.15, &c.accent, "left"));
        cy += th;
    }
    for it in items {
        check_box(s, x + pad, cy + size * 0.05, size, &c.accent);
        let lines = crate::layout::estimate_lines(it, size, text_w, false) as f64;
        s.push(text_box(c, "Point", it, [x + pad + size * 1.5, cy - size * 0.12, text_w, lines * line + size], size, &c.fg, false, "left"));
        cy += lines * line + size * 0.55;
    }
    h
}

/// The services row: circles with icons and labels; returns its height.
fn services(c: &Ctx, s: &mut Vec<Step>, x: f64, y: f64, w: f64, u: f64) -> f64 {
    let items: Vec<_> = c.brand.service_items.iter().filter(|i| !i.label.trim().is_empty()).take(4).collect();
    if items.is_empty() {
        return 0.0;
    }
    let n = items.len() as f64;
    let col = w / n;
    let d = (120.0 * u).min(col * 0.62);
    let label = 26.0 * u;
    for (i, it) in items.iter().enumerate() {
        let cx = x + col * (i as f64 + 0.5);
        s.push(cmd("shape.create", json!({"kind": "ellipse", "rect": [cx - d / 2.0, y, d, d], "fill": c.accent, "name": format!("{} icon disc", it.label)})));
        match c.brand.icon(it.icon.trim()).and_then(|p| image(p, [cx - d * 0.29, y + d * 0.21, d * 0.58, d * 0.58], Fit::Contain, &format!("{} icon", it.label), 1.0, 0.0)) {
            Some(img) => s.push(img),
            None => s.push(text_line(c, "Icon letter", &it.label.chars().next().unwrap_or('•').to_string(), cx, y + d * 0.66, d * 0.45, WHITE, true, "center")),
        }
        s.push(text_box(c, "Service", &it.label, [cx - col / 2.0 + 6.0, y + d + 14.0 * u, col - 12.0, label * 3.0], label, &c.accent, true, "center"));
    }
    d + 14.0 * u + label * 2.6
}

/// Contact footer bar across the bottom: location · phone · website.
fn footer(c: &Ctx, s: &mut Vec<Step>, f: &Frame, wide: bool) -> f64 {
    let b = c.brand;
    let items: Vec<(&str, String)> = [("pin", b.location.clone()), ("phone", b.phone.clone()), ("globe", b.website.clone())].into_iter().filter(|(_, t)| !t.trim().is_empty()).collect();
    if items.is_empty() {
        return 0.0;
    }
    let fh = (if wide { 84.0 } else { 116.0 }) * f.u;
    let y = f.h - fh;
    s.push(rect("Footer bar", [0.0, y, f.w, fh], json!(c.accent)));
    s.push(rect("Footer gold edge", [0.0, y, f.w, (5.0 * f.u).max(2.0)], json!(c.accent2)));
    let col = f.w / items.len() as f64;
    let size = (if wide { 26.0 } else { 30.0 }) * f.u;
    for (i, (icon, text)) in items.iter().enumerate() {
        let x0 = col * i as f64;
        if i > 0 {
            s.push(rect("Footer divider", [x0, y + fh * 0.24, (2.0 * f.u).max(1.0), fh * 0.52], json!(alpha(WHITE, 0.45))));
        }
        let is = size * 1.25;
        let tw = (text.chars().count() as f64 * size * (if *icon == "phone" { 0.72 } else { 0.6 })).min(col - is - 30.0 * f.u);
        let start = x0 + (col - is - 14.0 * f.u - tw).max(0.0) / 2.0;
        if let Some(p) = b.icon(icon)
            && let Some(img) = image(p, [start, y + (fh - is) / 2.0, is, is], Fit::Contain, &format!("{icon} icon"), 1.0, 0.0)
        {
            s.push(img);
        }
        let bold = *icon == "phone";
        s.push(text_box(c, "Contact", text, [start + is + 14.0 * f.u, y + (fh - size * 1.3) / 2.0, tw + 80.0 * f.u, size * 1.6], if bold { size * 1.15 } else { size }, WHITE, bold, "left"));
    }
    fh
}

/// One tall or square card: logo, tagline, kicker, headline, sub line, checklist, services, footer.
#[allow(clippy::too_many_arguments)]
fn hero(c: &Ctx, fmt: Format, kicker: &str, head: &str, sub: &str, card_title: &str, items: &[String], img: &str, show_services: bool) -> Vec<Step> {
    let f = Frame::of(fmt);
    let mut s = Vec::new();
    background(c, &mut s, &f, img, false);
    let (m, u) = (f.m, f.u);
    let lh = 150.0 * u;
    logo(c, &mut s, [m, m, 430.0 * u, lh]);
    if !c.brand.script_tagline.trim().is_empty() {
        s.push(script(c, "Script tagline", c.brand.script_tagline.trim(), [f.w * 0.5, m + 4.0 * u, f.w * 0.5 - m, 130.0 * u], 44.0 * u, &c.accent, "right"));
    }
    let footer_h = footer(c, &mut s, &f, false);
    let bottom = f.h - footer_h - m * 0.7;
    // Services sit just above the footer.
    let svc_h = if show_services && f.h >= f.w * 1.2 { services(c, &mut Vec::new(), m, 0.0, f.w - 2.0 * m, u) } else { 0.0 };
    let svc_y = bottom - svc_h;
    if svc_h > 0.0 {
        services(c, &mut s, m, svc_y, f.w - 2.0 * m, u);
    }
    let mut y = m + lh + 40.0 * u;
    y += band(c, &mut s, "Kicker", kicker, m, y, 34.0 * u, &c.accent, WHITE, f.w - 2.0 * m);
    y += 26.0 * u;
    let tall = f.h > f.w * 1.5;
    let room = svc_y - y;
    let card_guess = if items.is_empty() { 0.0 } else { (items.len() as f64 * 62.0 + 120.0) * u };
    let lines = if tall { 6 } else if room - card_guess > 520.0 * u { 5 } else { 4 };
    y += headline(c, &mut s, head, m, y, f.w - 2.0 * m, lines, 54.0 * u, if tall { 132.0 * u } else { 118.0 * u });
    if !sub.trim().is_empty() {
        let size = 40.0 * u;
        let sh = height_of(sub, size, f.w - 2.0 * m, true, 1.15);
        if y + sh + 20.0 * u < svc_y {
            s.push(display(c, "Sub line", &sub.to_uppercase(), [m, y + 10.0 * u, f.w - 2.0 * m, sh + size], size, &c.fg, "left"));
            y += sh + 34.0 * u;
        }
    }
    if !items.is_empty() {
        let cw = if f.h > f.w * 1.1 { f.w - 2.0 * m } else { (f.w - 2.0 * m) * 0.72 };
        let mut probe = Vec::new();
        let ch = checklist(c, &mut probe, card_title, items, m, y, cw, u);
        if y + ch <= svc_y - 12.0 * u {
            checklist(c, &mut s, card_title, items, m, y, cw, u);
        } else {
            // Not enough room: fewer points.
            let fewer: Vec<String> = items.iter().take(3).cloned().collect();
            let ch = checklist(c, &mut Vec::new(), card_title, &fewer, m, y, cw, u);
            if y + ch <= svc_y - 12.0 * u {
                checklist(c, &mut s, card_title, &fewer, m, y, cw, u);
            }
        }
    }
    s
}

/// Wide formats: text column on the left, photo on the right, footer bar.
fn wide(c: &Ctx, fmt: Format, kicker: &str, head: &str, sub: &str, img: &str) -> Vec<Step> {
    let f = Frame::of(fmt);
    let mut s = Vec::new();
    background(c, &mut s, &f, img, true);
    let u = f.h / 1080.0;
    let m = 70.0 * u;
    let col = f.w * if img.trim().is_empty() { 0.9 } else { 0.56 };
    let lh = 170.0 * u;
    logo(c, &mut s, [m, m, 480.0 * u, lh]);
    let footer_h = footer(c, &mut s, &Frame { w: f.w, h: f.h, u: u * 1.2, m }, true);
    let mut y = m + lh + 40.0 * u;
    y += band(c, &mut s, "Kicker", kicker, m, y, 40.0 * u, &c.accent, WHITE, col - m);
    y += 28.0 * u;
    let big = fmt == Format::YoutubeThumb;
    let lines = if big { 3 } else { 4 };
    y += headline(c, &mut s, head, m, y, col - m, lines, 60.0 * u, if big { 170.0 * u } else { 132.0 * u });
    if !sub.trim().is_empty() && !big {
        let size = 46.0 * u;
        if y + size * 2.0 < f.h - footer_h - m * 0.5 {
            s.push(display(c, "Sub line", &sub.to_uppercase(), [m, y + 12.0 * u, col - m, size * 2.6], size, &c.fg, "left"));
        }
    }
    if !c.brand.script_tagline.trim().is_empty() && !img.trim().is_empty() {
        s.push(script(c, "Script tagline", c.brand.script_tagline.trim(), [f.w * 0.58, m * 0.7, f.w * 0.42 - m, 160.0 * u], 52.0 * u, &c.accent, "right"));
    }
    s
}

/// YouTube channel banner: inside the centre 1546×423 safe area.
fn banner(c: &Ctx, plan: &Plan) -> Vec<Step> {
    let f = Frame::of(Format::YoutubeBanner);
    let mut s = Vec::new();
    let img = first_image(plan);
    if let Some(i) = image(&img, [0.0, 0.0, f.w, f.h], Fit::Cover, "Room photo", 1.0, 0.0) {
        s.push(i);
    }
    s.push(rect("Light wash", [0.0, 0.0, f.w, f.h], json!(alpha(WHITE, if img.trim().is_empty() { 0.0 } else { 0.72 }))));
    let (sx, sy, sw, sh) = ((f.w - 1546.0) / 2.0, (f.h - 423.0) / 2.0, 1546.0, 423.0);
    logo(c, &mut s, [sx, sy + 30.0, 520.0, sh - 60.0]);
    let tx = sx + 580.0;
    let tw = sw - 580.0;
    let head = [&plan.headline, &plan.title_card.headline].into_iter().find(|t| !t.trim().is_empty()).map_or(c.brand.pitch.as_str(), |t| t.trim());
    if !c.brand.script_tagline.trim().is_empty() {
        s.push(script(c, "Script tagline", c.brand.script_tagline.trim(), [tx, sy + 10.0, tw, 110.0], 64.0, &c.accent, "left"));
    }
    let h = headline(c, &mut s, head, tx, sy + 130.0, tw, 2, 48.0, 92.0);
    let line = [c.brand.phone.as_str(), c.brand.website.as_str()].iter().filter(|t| !t.trim().is_empty()).copied().collect::<Vec<_>>().join("   ·   ");
    if !line.is_empty() {
        s.push(rounded("Contact band", [tx, sy + 150.0 + h, (line.chars().count() as f64 * 26.0 + 80.0).min(tw), 76.0], 18.0, json!(c.accent)));
        s.push(text_line(c, "Contact", &line, tx + 40.0, sy + 150.0 + h + 52.0, 40.0, WHITE, true, "left"));
    }
    s
}

fn outro(c: &Ctx, plan: &Plan, fmt: Format) -> Vec<Step> {
    let f = Frame::of(fmt);
    let (m, u) = (f.m, f.u);
    let mut s = Vec::new();
    background(c, &mut s, &f, "", false);
    let footer_h = footer(c, &mut s, &f, false);
    let lw = f.w * 0.62;
    logo(c, &mut s, [(f.w - lw) / 2.0, m * 1.4, lw, 220.0 * u]);
    let mut y = m * 1.4 + 260.0 * u;
    if !c.brand.script_tagline.trim().is_empty() {
        s.push(script(c, "Script tagline", c.brand.script_tagline.trim(), [m, y, f.w - 2.0 * m, 110.0 * u], 54.0 * u, &c.accent, "center"));
        y += 120.0 * u;
    }
    let cta = if plan.outro_card.cta.trim().is_empty() { "Book your clean today" } else { plan.outro_card.cta.trim() };
    let size = fit_size(&cta.to_uppercase(), f.w - 2.0 * m, 3, 50.0 * u, 96.0 * u, true);
    let hh = height_of(&cta.to_uppercase(), size, f.w - 2.0 * m, true, 1.06);
    s.push(display(c, "Call to action", &cta.to_uppercase(), [m, y, f.w - 2.0 * m, hh + size], size, &c.accent, "center"));
    y += hh + 40.0 * u;
    if !c.brand.phone.trim().is_empty() {
        let pw = 640.0 * u;
        s.push(rounded("Call pill", [(f.w - pw) / 2.0, y, pw, 130.0 * u], 65.0 * u, json!(c.accent2)));
        s.push(text_line(c, "Call now", "CALL NOW", f.w / 2.0, y + 46.0 * u, 26.0 * u, &c.accent, true, "center"));
        s.push(text_line(c, "Phone", c.brand.phone.trim(), f.w / 2.0, y + 106.0 * u, 56.0 * u, &c.accent, true, "center"));
        y += 170.0 * u;
    }
    if !c.brand.address.trim().is_empty() {
        s.push(text_box(c, "Address", c.brand.address.trim(), [m, y, f.w - 2.0 * m, 80.0 * u], 30.0 * u, &c.fg, false, "center"));
    }
    let bottom = f.h - footer_h - m * 0.7;
    let svc_h = services(c, &mut Vec::new(), m, 0.0, f.w - 2.0 * m, u);
    if svc_h > 0.0 && bottom - svc_h > y + 90.0 * u {
        services(c, &mut s, m, bottom - svc_h, f.w - 2.0 * m, u);
    }
    s
}

fn content(c: &Ctx, n: usize, cs: &ContentSlide, fmt: Format) -> Vec<Step> {
    let items = bullets(&cs.body);
    hero(c, fmt, &format!("{n:02}"), &cs.heading, "", "", &items, &cs.image, false)
}

/// Every slide of `plan` in the Service look.
pub(crate) fn slides(c: &Ctx, plan: &Plan, fmt: Format) -> Vec<SlideSpec> {
    let img = first_image(plan);
    let head = [&plan.headline, &plan.title_card.headline, &plan.quote].into_iter().find(|t| !t.trim().is_empty()).map_or("", |t| t.trim()).to_string();
    let sub = [&plan.subhead, &plan.title_card.subhead].into_iter().find(|t| !t.trim().is_empty()).map_or("", |t| t.trim()).to_string();
    let kicker = c.brand.category.clone();
    let one = |steps: Vec<Step>| vec![spec(format!("01_{}", fmt.key()), SlideKind::Single, fmt, &c.bg, steps)];
    match fmt {
        Format::Carousel => {
            let mut out = vec![spec("01_title".into(), SlideKind::Title, fmt, &c.bg, hero(c, fmt, &kicker, &head, &sub, "", &[], &img, true))];
            for (i, cs) in plan.content_slides.iter().take(8).enumerate() {
                out.push(spec(format!("{:02}_content", i + 2), SlideKind::Content, fmt, &c.bg, content(c, i + 1, cs, fmt)));
            }
            let n = out.len() + 1;
            out.push(spec(format!("{n:02}_outro"), SlideKind::Outro, fmt, &c.bg, outro(c, plan, fmt)));
            out
        }
        Format::YoutubeBanner => one(banner(c, plan)),
        Format::Cover => one(outro(c, plan, fmt)),
        f if f.landscape() => one(wide(c, f, &kicker, &head, &sub, &img)),
        f => {
            let items = bullets(&plan.body);
            let card = if items.is_empty() { String::new() } else { "What you get".to_string() };
            one(hero(c, f, &kicker, &head, if items.is_empty() { &sub } else { "" }, &card, &items, &img, true))
        }
    }
}
