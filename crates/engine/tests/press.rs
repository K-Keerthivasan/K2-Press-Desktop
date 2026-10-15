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
    assert_eq!(s.execute("press.formats", json!({})).unwrap()["formats"].as_array().unwrap().len(), 6);
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
    for (style, b) in [("classic", json!({"name": "K2 Digital Media", "handle": "@k2digitalmedia_", "tagline": "The complete online presence for local business", "website": "k2digitalmedia.ca", "services": "Web · Video · Marketing · IT", "logo": logo, "category": "WEB DEVELOPMENT"})),
                       ("imageForward", json!({"name": "JKR", "handle": "@jkr_studio_", "category": "GAMING", "image_forward": true, "theme": {"navy": "#0B0B0F", "navy2": "#141418", "accent": "#ED1C24", "accent2": "#FF5A4D", "text": "#FFFFFF"}}))] {
        for f in ["carousel", "square", "story", "x", "quote", "cover"] {
            let mut s = Session::new();
            let r = s.execute("press.build", json!({"plan": p, "brand": b, "format": f, "style": style})).unwrap();
            for (i, d) in r["documents"].as_array().unwrap().iter().enumerate() {
                eprintln!("{style} {f} {}: {:?}", d["slide"], d["warnings"]);
                s.set_active(i);
                let out = format!("{dir}/{style}_{f}_{}.png", d["slide"].as_str().unwrap());
                s.execute("file.saveACopy", json!({"path": out})).unwrap();
            }
        }
    }
}
