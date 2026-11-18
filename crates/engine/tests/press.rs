//! Post Studio: `press.build` turns a plan into editable documents.

use photocraft_engine::Session;
use serde_json::{Value, json};

fn plan() -> Value {
    json!({
        "slug": "ux-friction",
        "title_card": {"headline": "UX friction is quietly draining your sales", "subhead": "Three fixes that pay back."},
        "content_slides": [{"heading": "SLOW PAGES", "body": "— Every second costs bookings\n— Compress images"}],
        "outro_card": {"cta": "Save this", "handle": "@k2"},
        "caption": "c", "hashtags": ["ux"], "dm_keyword": "audit"
    })
}

fn brand() -> Value {
    json!({"name": "K2", "handle": "@k2", "theme": {"navy": "#0A0F1E", "accent": "#00B4C8"}})
}

fn temp_png(name: &str, w: u32, h: u32) -> String {
    let dir = std::env::temp_dir().join(format!("press-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name).to_string_lossy().replace('\\', "/");
    let mut s = Session::new();
    s.execute("file.new", json!({"width": w, "height": h, "background": "#ff0000"})).unwrap();
    s.execute("file.saveACopy", json!({"path": path})).unwrap();
    path
}

#[test]
fn builds_one_document_per_slide_with_layers() {
    let mut s = Session::new();
    let r = s.execute("press.build", json!({"plan": plan(), "brand": brand()})).unwrap();
    let docs = r["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 3);
    assert_eq!(s.documents().len(), 3);
    for d in docs {
        assert!(d["warnings"].as_array().unwrap().is_empty(), "{d}");
    }
    let title = &s.documents()[0].doc;
    assert_eq!((title.size.width, title.size.height), (1080, 1350));
    let names: Vec<String> = title.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
    assert!(names.iter().any(|n| n == "Headline"), "{names:?}");
    assert!(names.iter().any(|n| n == "Accent rule"), "{names:?}");
    // The first slide is active afterwards.
    assert_eq!(s.active_index(), Some(0));
}

#[test]
fn photo_is_cover_fitted_and_clipped_to_its_frame() {
    let img = temp_png("wide.png", 400, 100);
    let mut p = plan();
    p["content_slides"][0]["image"] = json!(img);
    let mut s = Session::new();
    let r = s.execute("press.build", json!({"plan": p, "brand": brand(), "slides": [1]})).unwrap();
    let d = &r["documents"][0];
    assert!(d["warnings"].as_array().unwrap().is_empty(), "{d}");
    let doc = &s.documents()[0].doc;
    let photo = doc.walk().into_iter().map(|(_, _, l)| l).find(|l| l.name == "Slide photo").expect("photo layer");
    assert!(photo.clipped, "the photo clips to its frame");
    assert!(doc.walk().into_iter().map(|(_, _, l)| l).any(|l| l.name == "Slide photo frame"));
}

#[test]
fn missing_photo_is_a_warning_not_a_failure() {
    let mut p = plan();
    p["content_slides"][0]["image"] = json!("Z:/definitely/missing.png");
    let mut s = Session::new();
    let r = s.execute("press.build", json!({"plan": p, "brand": brand(), "slides": [1]})).unwrap();
    let w = r["documents"][0]["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 1, "{w:?}");
    let doc = &s.documents()[0].doc;
    assert!(!doc.walk().into_iter().map(|(_, _, l)| l).any(|l| l.name.contains("frame")), "no empty frame left behind");
}

#[test]
fn bad_params_are_errors() {
    let mut s = Session::new();
    assert!(s.execute("press.build", json!({})).is_err());
    assert!(s.execute("press.build", json!({"plan": 5})).is_err());
    assert!(s.execute("press.build", json!({"plan": plan(), "slides": [99]})).is_err());
    assert!(s.execute("press.build", json!({"plan": plan(), "slides": "x"})).is_err());
    assert!(s.execute("press.build", json!({"plan": plan(), "style": "neon"})).is_err());
    assert!(s.execute("press.buildSpec", json!({"spec": {"name": "x"}})).is_err());
    let huge = json!({"spec": {"name": "x", "kind": "single", "width": 100000, "height": 10, "background": "#000", "steps": []}});
    assert!(s.execute("press.buildSpec", huge).is_err());
    let sneaky = json!({"spec": {"name": "x", "kind": "single", "width": 10, "height": 10, "background": "#000", "steps": [{"step": "cmd", "id": "file.close", "params": {}}]}});
    let r = s.execute("press.buildSpec", sneaky).unwrap();
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(s.documents().len(), 1);
}

#[test]
fn every_format_builds() {
    let mut s = Session::new();
    for f in ["carousel", "square", "story", "x", "quote", "cover"] {
        let r = s.execute("press.build", json!({"plan": plan(), "brand": brand(), "format": f})).unwrap();
        for d in r["documents"].as_array().unwrap() {
            assert!(d["warnings"].as_array().unwrap().is_empty(), "{f}: {d}");
        }
    }
    let q = s.execute("press.slides", json!({"plan": plan(), "format": "story"})).unwrap();
    assert_eq!(q["slides"][0]["height"], 1920);
    assert_eq!(s.execute("press.formats", json!({})).unwrap()["formats"].as_array().unwrap().len(), photocraft_press::Format::ALL.len());
}

/// Writes every format to `PRESS_PREVIEW_DIR` as PNGs for a visual check:
/// `PRESS_PREVIEW_DIR=… PRESS_PREVIEW_IMAGE=… cargo test -p photocraft-engine --test press preview -- --ignored`.
#[test]
#[ignore]
fn preview() {
    let Ok(dir) = std::env::var("PRESS_PREVIEW_DIR") else { return };
    let img = std::env::var("PRESS_PREVIEW_IMAGE").unwrap_or_default();
    let logo = std::env::var("PRESS_PREVIEW_LOGO").unwrap_or_default();
    let mut p = plan();
    p["image"] = json!(img);
    p["content_slides"][0]["image"] = json!(img);
    p["quote"] = json!("The best interface is the one your customer never notices.");
    p["subhead"] = json!("Jakob Nielsen");
    for (style, b) in [
        (
            "classic",
            json!({"name": "K2 Digital Media", "handle": "@k2digitalmedia_", "tagline": "The complete online presence for local business", "website": "k2digitalmedia.ca", "services": "Web · Video · Marketing · IT", "logo": logo, "category": "WEB DEVELOPMENT"}),
        ),
        (
            "imageForward",
            json!({"name": "JKR", "handle": "@jkr_studio_", "category": "GAMING", "image_forward": true, "theme": {"navy": "#0B0B0F", "navy2": "#141418", "accent": "#ED1C24", "accent2": "#FF5A4D", "text": "#FFFFFF"}}),
        ),
    ] {
        for f in ["carousel", "square", "story", "x", "quote", "cover"] {
            let mut s = Session::new();
            let r = s.execute("press.build", json!({"plan": p, "brand": b, "format": f, "style": style})).unwrap();
            let mut outs = Vec::new();
            for (i, d) in r["documents"].as_array().unwrap().iter().enumerate() {
                eprintln!("{style} {f} {}: {:?}", d["slide"], d["warnings"]);
                s.set_active(i);
                let out = format!("{dir}/{style}_{f}_{}.png", d["slide"].as_str().unwrap());
                s.execute("file.saveACopy", json!({"path": out})).unwrap();
                outs.push(out);
            }
            let look = if style == "classic" { "Classic look" } else { "Image-forward look" };
            seed_gallery(b["name"].as_str().unwrap(), look, f, &outs);
        }
    }
}

/// The Service look (Phoenix Fine Touch) in every format, to `PRESS_PREVIEW_DIR`:
/// `PRESS_PREVIEW_DIR=… PRESS_HOME=… PRESS_PREVIEW_IMAGE=… cargo test -p photocraft-engine --test press preview_service -- --ignored`.
#[test]
#[ignore]
fn preview_service() {
    let (Ok(dir), Ok(home)) = (std::env::var("PRESS_PREVIEW_DIR"), std::env::var("PRESS_HOME")) else { return };
    let img = std::env::var("PRESS_PREVIEW_IMAGE").unwrap_or_default();
    let icons: serde_json::Map<String, Value> =
        ["house", "leaf", "sparkle", "shield", "pin", "phone", "globe"].iter().map(|n| (n.to_string(), json!(format!("{home}/static/icons/{n}.svg")))).collect();
    let brand = json!({
        "name": "Phoenix Fine Touch", "handle": "@phoenixfinetouch", "kind": "client", "style": "service",
        "tagline": "Cleaning Services London Ontario", "script_tagline": "The fine touch. Every time.",
        "category": "Residential & Commercial Cleaning", "location": "London, Ontario & Surrounding Areas", "phone": "519-999-5000",
        "website": "phoenixfinetouch.ca", "address": "317 Adelaide St S Unit 113, London, ON N5Z 3L3",
        "font": "Segoe UI Black", "logo_shape": "wide", "logo": format!("{home}/static/logo_phoenix.png"), "icons": icons,
        "service_items": [{"label": "Top to Bottom Detailed Cleaning", "icon": "sparkle"}, {"label": "Organic & Eco-Friendly Options", "icon": "leaf"}, {"label": "Fully Insured & Bonded", "icon": "shield"}],
        "theme": {"navy": "#F4F1EA", "navy2": "#FFFFFF", "accent": "#0B5557", "accent2": "#E4B039", "text": "#123E40"}
    });
    let plan = json!({
        "slug": "cleaner-spaces", "image": img, "headline": "Cleaner spaces. Better everyday.", "subhead": "A little more care. A lot more attention to detail.",
        "body": "— Top to bottom detailed cleaning\n— Organic & eco-friendly options\n— Health, pet & children friendly\n— Fully insured & bonded",
        "title_card": {"headline": "Cleaner spaces. Better everyday.", "subhead": "A little more care. A lot more attention to detail."},
        "content_slides": [{"heading": "The details make the difference", "body": "— Top to bottom detailed cleaning\n— Organic & eco-friendly options\n— Health, pet & children friendly", "image": img}],
        "outro_card": {"cta": "Request your free quote"}, "caption": "c", "hashtags": ["PhoenixFineTouch"]
    });
    for f in ["carousel", "portrait", "square", "story", "facebook", "linkedin", "x", "youtube_thumb", "youtube_banner", "email", "pinterest"] {
        let mut s = Session::new();
        let r = s.execute("press.build", json!({"plan": plan, "brand": brand, "format": f})).unwrap();
        let mut outs = Vec::new();
        for (i, d) in r["documents"].as_array().unwrap().iter().enumerate() {
            eprintln!("{f} {}: {:?}", d["slide"], d["warnings"]);
            s.set_active(i);
            let out = format!("{dir}/svc_{f}_{}.png", d["slide"].as_str().unwrap());
            s.execute("file.saveACopy", json!({"path": out})).unwrap();
            outs.push(out);
        }
        seed_gallery("Phoenix Fine Touch", "Service look", f, &outs);
    }
}

/// With `PRESS_SEED_GALLERY=1`, the renders become the channel's templates in the shared
/// "Channel template gallery" (one row per look and platform, examples = these renders).
fn seed_gallery(account: &str, look: &str, format: &str, examples: &[String]) {
    if std::env::var("PRESS_SEED_GALLERY").is_err() {
        return;
    }
    let fmt = photocraft_press::Format::from_key(format);
    let (w, h) = fmt.size();
    let description = "{title}\n\n{summary}\n\n{creator}";
    let skills = format!(
        "{} in the {look} ({w}×{h}). Build it from Post Studio: pick this channel, choose the {} format and the brand's look; every headline, band and photo is an editable layer. Keep copy polite and on-brand.",
        fmt.platform(),
        fmt.label()
    );
    match k2_format_gallery::seed_template(account, look, fmt.label(), description, &skills, examples) {
        Ok(f) => eprintln!("gallery: {account} / {look} / {} rev {}", f.name, f.revision),
        Err(e) => panic!("gallery seed {account} {format}: {e}"),
    }
}
