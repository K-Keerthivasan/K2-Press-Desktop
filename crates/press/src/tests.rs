#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use crate::csv::{detect_shape, parse, to_brief, to_plan, to_posts};
use crate::layout::{Format, SlideKind, Step, Style, estimate_lines, fit_size, mix, slides};
use crate::text::{dedupe, similarity, slug, split_tags};
use crate::{Brand, Plan};

fn brand() -> Brand {
    Brand::from_value(
        "k2",
        &json!({"name": "K2 Digital Media", "handle": "@k2digitalmedia_", "tagline": "Online presence", "hashtags": ["london", "ontario"],
                "theme": {"navy": "#0A0F1E", "navy2": "#0C1426", "accent": "#00B4C8", "accent2": "#00C896", "text": "#FFFFFF"},
                "feeds": {"categories": {"tech": {"name": "Tech", "urls": ["https://example.com/feed"]}}}, "video": {"enabled": true}}),
    )
}

fn plan() -> Plan {
    Plan::from_json(&json!({
        "slug": "ux-friction", "slide_count": 4,
        "title_card": {"headline": "UX friction is quietly draining your sales", "subhead": "Three fixes that pay back for years."},
        "content_slides": [
            {"heading": "SLOW PAGES", "body": "— Every second costs bookings\n— Compress images first", "image_query": "laptop"},
            {"heading": "HIDDEN PRICES", "body": "— Show the price\n— Trust converts", "image_query": "shop"}],
        "outro_card": {"cta": "Save this for your next audit", "handle": "@k2digitalmedia_"},
        "caption": "Small fixes, big wins.", "hashtags": ["ux", "#web"], "dm_keyword": "audit", "custom_key": 7}))
    .unwrap()
}

#[test]
fn brand_reads_config_block_and_keeps_extra_keys() {
    let b = brand();
    assert_eq!(b.key, "k2");
    assert_eq!(b.at_handle(), "@k2digitalmedia_");
    assert!(b.extra.contains_key("video"));
    let cats = b.feed_categories();
    assert_eq!(cats.len(), 1);
    assert_eq!(cats[0].1, "Tech");
    assert!(cats[0].2);
}

#[test]
fn brand_lenient_on_bad_types() {
    let b = Brand::from_value("x", &json!({"name": 5, "hashtags": "seo, local #web", "image_forward": true}));
    assert_eq!(b.key, "x");
    assert_eq!(b.hashtags, vec!["seo", "local", "web"]);
    assert!(b.image_forward);
}

#[test]
fn plan_round_trips_unknown_keys() {
    let p = plan();
    assert_eq!(p.extra.get("custom_key"), Some(&json!(7)));
    let back = Plan::from_json(&p.to_json()).unwrap();
    assert_eq!(back, p);
    assert_eq!(p.title(), "UX friction is quietly draining your sales");
}

#[test]
fn plan_rejects_non_object() {
    assert!(Plan::from_json(&json!([1, 2])).is_err());
}

#[test]
fn caption_appends_tags_and_caps_length() {
    let p = plan();
    assert_eq!(p.full_caption(), "Small fixes, big wins.\n\n#ux #web");
    let long = Plan { caption: "x".repeat(3000), ..Plan::default() };
    assert_eq!(long.full_caption().chars().count(), 2200);
}

#[test]
fn carousel_has_title_content_outro() {
    let s = slides(&plan(), &brand(), None, None);
    assert_eq!(s.len(), 4);
    assert_eq!(s[0].name, "01_title");
    assert_eq!(s[1].name, "02_content");
    assert_eq!(s[3].name, "04_outro");
    assert_eq!(s[3].kind, SlideKind::Outro);
    assert!(s.iter().all(|x| x.width == 1080 && x.height == 1350));
    // Every step is a known command or an image placement.
    for sl in &s {
        for st in &sl.steps {
            if let Step::Cmd { id, .. } = st {
                assert!(["shape.create", "type.create"].contains(&id.as_str()), "{id}");
            }
        }
    }
}

#[test]
fn images_only_when_present() {
    let mut p = plan();
    let no_img = slides(&p, &brand(), None, Some(Style::ImageForward));
    assert!(no_img.iter().all(|s| s.steps.iter().all(|st| !matches!(st, Step::Image { .. }))));
    p.content_slides[0].image = "C:/img/a.jpg".into();
    let with = slides(&p, &brand(), None, Some(Style::ImageForward));
    assert!(with[1].steps.iter().any(|st| matches!(st, Step::Image { path, .. } if path == "C:/img/a.jpg")));
}

#[test]
fn single_formats_have_their_size() {
    let b = brand();
    let p = plan();
    for f in Format::ALL {
        let s = slides(&p, &b, Some(f), None);
        assert!(!s.is_empty());
        assert_eq!((s[0].width, s[0].height), f.size(), "{f:?}");
    }
    assert_eq!(Format::from_key("xpost"), Format::X);
    assert_eq!(Format::from_key("???"), Format::Carousel);
    assert_eq!(Format::Carousel.post_type(4), "carousel");
    assert_eq!(Format::Story.post_type(1), "story");
}

#[test]
fn empty_plan_never_panics() {
    let b = Brand::default();
    for f in Format::ALL {
        for st in [Style::Classic, Style::ImageForward] {
            let _ = slides(&Plan::default(), &b, Some(f), Some(st));
        }
    }
}

#[test]
fn text_fitting_shrinks_long_copy() {
    assert_eq!(estimate_lines("", 40.0, 800.0, false), 1);
    assert!(estimate_lines(&"word ".repeat(60), 40.0, 800.0, false) > 3);
    let short = fit_size("Hi", 900.0, 3, 40.0, 80.0, true);
    let long = fit_size(&"longer headline words ".repeat(8), 900.0, 3, 40.0, 80.0, true);
    assert_eq!(short, 80.0);
    assert!(long < short);
    // A single unbreakable word longer than the box still terminates.
    assert!(estimate_lines(&"x".repeat(500), 10.0, 1.0, true) > 1);
}

#[test]
fn colour_mix() {
    assert_eq!(mix("#000000", "#ffffff", 0.5), "#808080");
    assert_eq!(mix("nothex", "#ffffff", 0.5), "#ffffff");
    assert_eq!(mix("#fff", "#000000", 0.0), "#ffffff");
}

#[test]
fn text_helpers() {
    assert_eq!(slug("UX Friction: is it Real?!", "post"), "ux-friction-is-it-real");
    assert_eq!(slug("!!!", "post"), "post");
    assert_eq!(split_tags("#a, b  c"), vec!["a", "b", "c"]);
    assert!(similarity("Nintendo announces Switch 2 price", "Switch 2 price announced by Nintendo") > 0.6);
    let kept = dedupe(&["Nintendo Switch 2 price revealed".into(), "Switch 2 price revealed by Nintendo".into(), "Apple ships new MacBook".into()]);
    assert_eq!(kept, vec![0, 2]);
}

#[test]
fn csv_wide_sheet_direct_plan() {
    let (h, rows) = parse(crate::csv::TEMPLATE_CSV).unwrap();
    assert_eq!(detect_shape(&h), "wide");
    let posts = to_posts(&h, &rows);
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].slides.len(), 3);
    assert_eq!(posts[0].hashtags, vec!["seo", "localbusiness", "marketing"]);
    assert!(posts[0].slides[0].body.contains('\n'));
    let p = to_plan(&posts[0], &brand(), "");
    assert_eq!(p.format, "carousel");
    assert_eq!(p.content_slides.len(), 3);
    assert_eq!(p.outro_card.handle, "@k2digitalmedia_");
    let sq = to_plan(&posts[0], &brand(), "square");
    assert_eq!(sq.headline, "5 SEO myths that cost you leads");
}

#[test]
fn csv_long_sheet_groups_and_orders() {
    let text = "post_id;order;heading;body\nseo;2;B;second\nseo;1;A;first\nweb;1;C;third\n";
    let (h, rows) = parse(text).unwrap();
    assert_eq!(detect_shape(&h), "long");
    let posts = to_posts(&h, &rows);
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].slides[0].heading, "A");
    assert_eq!(posts[1].title, "web");
    let (title, notes) = to_brief(&posts[0]);
    assert_eq!(title, "seo");
    assert!(notes.contains("A: first"));
}

#[test]
fn csv_errors_are_errors() {
    assert!(parse("").is_err());
    assert!(parse("a,b\n\"unterminated").is_err());
    let (h, rows) = parse("\u{feff}Title,Notes\r\nIdea one,some notes\r\n").unwrap();
    let posts = to_posts(&h, &rows);
    assert_eq!(posts[0].notes, "some notes");
}
