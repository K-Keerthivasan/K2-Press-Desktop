//! Built-in icons for the Service look (the services row and the contact footer): simple line
//! glyphs drawn for this app, written once as white SVGs into `<K2 Press>/static/icons/`. A brand
//! can use its own artwork by putting `<name>.svg` or `<name>.png` there first; existing files are
//! never overwritten.

use std::path::Path;

use photocraft_press::Brand;

use crate::home::Home;

/// (name, SVG body inside a 24×24 view box, stroked).
const GLYPHS: &[(&str, &str)] = &[
    ("house", r#"<path d="M3 11.5 12 4l9 7.5"/><path d="M5.5 10v10h13V10"/><path d="M10 20v-5.5h4V20"/>"#),
    ("leaf", r#"<path d="M5 19C5 10.5 10.5 5 20 4c-.8 9.4-6.2 15-15 15Z"/><path d="M5 19l8.5-8.5"/>"#),
    ("sparkle", r#"<path d="M12 3l2.1 6.4L20.5 12l-6.4 2.1L12 20.5l-2.1-6.4L3.5 12l6.4-2.6Z"/><path d="M19 3v3M17.5 4.5h3"/>"#),
    ("shield", r#"<path d="M12 3l8 3v6c0 4.8-3.4 8-8 9-4.6-1-8-4.2-8-9V6Z"/><path d="M8.5 12.3l2.4 2.4 4.6-4.9"/>"#),
    ("star", r#"<path d="M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.8l-5.2 2.8 1-5.8-4.3-4.1 5.9-.8Z"/>"#),
    ("check", r#"<path d="M5 12.5l4.5 4.5L19 7"/>"#),
    ("broom", r#"<path d="M15.5 3 11 11.5"/><path d="M7 11.5h8l2.5 9h-13Z"/><path d="M9 15.5v5M12 15.5v5M15 15.5v5"/>"#),
    ("spray", r#"<path d="M8 8h5v13H8Z"/><path d="M9 8V5h3v3"/><path d="M12 5h3l1.5-1.5M17 6.5h3M17 9.5l2.5 1M17 3.5 19.5 2.5"/>"#),
    ("calendar", r#"<path d="M4 6h16v14H4Z"/><path d="M4 10h16M8 3.5v4M16 3.5v4"/>"#),
    ("pin", r#"<path d="M12 21s7-6.2 7-12a7 7 0 0 0-14 0c0 5.8 7 12 7 12Z"/><circle cx="12" cy="9" r="2.6"/>"#),
    ("phone", r#"<path d="M5.5 4h3.6l1.9 4.8-2.4 1.5a11.5 11.5 0 0 0 5.1 5.1l1.5-2.4L20 15v3.6a2 2 0 0 1-2.2 2A16.5 16.5 0 0 1 3.4 6.2 2 2 0 0 1 5.5 4Z"/>"#),
    ("globe", r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3c2.6 2.6 3.8 5.7 3.8 9S14.6 18.4 12 21c-2.6-2.6-3.8-5.7-3.8-9S9.4 5.6 12 3Z"/>"#),
    ("mail", r#"<path d="M3.5 6h17v12h-17Z"/><path d="m4 7 8 6 8-6"/>"#),
];

/// The SVG for `name` (white strokes, rasterised at 512 px so it stays sharp when scaled).
pub fn svg(name: &str) -> Option<String> {
    let body = GLYPHS.iter().find(|(n, _)| *n == name)?.1;
    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
    ))
}

pub fn names() -> impl Iterator<Item = &'static str> {
    GLYPHS.iter().map(|(n, _)| *n)
}

/// Fill `brand.icons` with paths for its services row and the contact footer, writing missing
/// built-in icons into `static/icons/`. Unknown names fall back to `check`.
pub fn resolve(home: &Home, brand: &mut Brand) {
    let dir = home.root.join("static").join("icons");
    let mut wanted: Vec<String> = brand.service_items.iter().map(|s| s.icon.trim().to_string()).filter(|s| !s.is_empty()).collect();
    wanted.extend(["pin", "phone", "globe", "mail"].map(str::to_string));
    for name in wanted {
        if let Some(p) = existing(&dir, &name) {
            brand.icons.insert(name, p);
            continue;
        }
        let glyph = if svg(&name).is_some() { name.clone() } else { "check".to_string() };
        let path = dir.join(format!("{glyph}.svg"));
        if !path.is_file()
            && let Some(text) = svg(&glyph)
        {
            let _ = std::fs::create_dir_all(&dir);
            if std::fs::write(&path, text).is_err() {
                continue;
            }
        }
        brand.icons.insert(name, path.to_string_lossy().replace('\\', "/"));
    }
}

fn existing(dir: &Path, name: &str) -> Option<String> {
    ["png", "svg"].iter().map(|ext| dir.join(format!("{name}.{ext}"))).find(|p| p.is_file()).map(|p| p.to_string_lossy().replace('\\', "/"))
}
