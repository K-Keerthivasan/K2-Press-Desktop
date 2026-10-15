#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use crate::config::Config;
use crate::home::{Home, Settings, parse_env};
use crate::library;
use crate::llm::{Backend, clean_json};
use crate::schedule::{month_grid, next_slots, parse_when, shift_month};

fn temp_home(config: &str) -> Home {
    let dir = std::env::temp_dir().join(format!("studio-test-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(dir.join("library")).unwrap();
    std::fs::write(dir.join("config.yaml"), config).unwrap();
    Home::new(dir).unwrap()
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

const CONFIG: &str = r##"
active_brand: k2
meta:
  accounts:
    k2: {ig_user_id: "123", targets: ["instagram"]}
schedule:
  times: ["09:00", "18:00"]
  days: ["mon", "wed"]
brands:
  k2:
    name: "K2 Digital Media"
    handle: "@k2"
    logo_path: "static/missing.png"
    hashtags: [a, b]
    theme: {navy: "#0A0F1E", accent: "#00B4C8"}
  jkr:
    name: JKR
    image_forward: true
llm:
  base_url: "http://localhost:11434/v1"
  model: "qwen3:8b"
  backends:
    lmstudio: {base_url: "http://localhost:1234/v1", model: "local"}
"##;

#[test]
fn config_reads_brands_and_paths() {
    let home = temp_home(CONFIG);
    let cfg = Config::load(&home).unwrap();
    assert_eq!(cfg.active_brand, "k2");
    assert_eq!(cfg.brands.len(), 2);
    let k2 = cfg.brand("k2").unwrap();
    assert_eq!(k2.hashtags, vec!["a", "b"]);
    assert!(k2.logo.is_none(), "missing logo file is not used");
    assert!(cfg.brand("jkr").unwrap().image_forward);
    assert_eq!(cfg.str_at("llm.model").as_deref(), Some("qwen3:8b"));
    assert!(Config::parse("brands: [", &home).is_err());
}

#[test]
fn backend_resolution() {
    let home = temp_home(CONFIG);
    let cfg = Config::load(&home).unwrap();
    let env = home.env();
    let b = Backend::resolve(&cfg, &env, &Settings::default());
    assert!(matches!(&b, Backend::OpenAi { model, .. } if model == "qwen3:8b"));
    let h = Backend::resolve(&cfg, &env, &Settings { backend: "hermes".into(), ..Default::default() });
    assert!(matches!(h, Backend::Hermes { .. }));
    let l = Backend::resolve(&cfg, &env, &Settings { backend: "lmstudio".into(), model: "x".into(), ..Default::default() });
    assert!(matches!(&l, Backend::OpenAi { base_url, model, .. } if base_url == "http://localhost:1234/v1" && model == "x"));
    assert!(Backend::names(&cfg).contains(&"lmstudio".to_string()));
}

#[test]
fn env_file_parsing() {
    let m = parse_env("# c\nA=1\nexport B=\"two words\"\nC='x' \nD=val # note\nbad line\n");
    assert_eq!(m.get("A").unwrap(), "1");
    assert_eq!(m.get("B").unwrap(), "two words");
    assert_eq!(m.get("C").unwrap(), "x");
    assert_eq!(m.get("D").unwrap(), "val");
    assert!(!m.contains_key("bad line"));
}

#[test]
fn json_cleanup() {
    assert_eq!(clean_json("<think>hmm {</think>\n{\"a\": 1}").unwrap(), json!({"a": 1}));
    assert_eq!(clean_json("Sure! ```json\n{\"a\": [1]}\n```").unwrap(), json!({"a": [1]}));
    assert_eq!(clean_json("[{\"index\":0}]").unwrap(), json!([{"index": 0}]));
    assert!(clean_json("no json here").is_err());
}

#[test]
fn queue_round_trip_keeps_unknown_fields() {
    let home = temp_home(CONFIG);
    std::fs::write(home.library().join("review_queue.json"), r#"[{"id":"old","status":"pending","weird":{"x":1}}]"#).unwrap();
    let plan = photocraft_press::Plan::from_json(&json!({"title_card": {"headline": "Hello"}, "caption": "c", "hashtags": ["t"], "source_url": "https://s/1"})).unwrap();
    let (rel, dir) = library::new_output_dir(&home, "k2", "hello", "carousel").unwrap();
    assert!(dir.is_dir());
    let (rel2, _) = library::new_output_dir(&home, "k2", "hello", "carousel").unwrap();
    assert_ne!(rel, rel2);
    let e = library::add_post(&home, "k2", &plan, &rel, &["01_title.png".into()]).unwrap();
    assert_eq!(library::s(&e, "caption"), "c\n\n#t");
    let q = library::load_queue(&home).unwrap();
    assert_eq!(q.len(), 2);
    assert_eq!(q[0].get("weird"), Some(&json!({"x": 1})));
    let id = library::s(&e, "id").to_string();
    library::update_entry(&home, &id, |e| {
        e.insert("status".into(), json!("approved"));
        Ok(())
    })
    .unwrap();
    assert_eq!(library::s(&library::load_queue(&home).unwrap()[1], "status"), "approved");
    assert!(library::entry_plan(&home, &e).is_some());
    assert!(library::used_urls(&home).contains("https://s/1"));
    library::remove_entry(&home, &id).unwrap();
    assert!(library::remove_entry(&home, &id).is_err());
}

#[test]
fn damaged_queue_is_an_error_not_an_empty_queue() {
    let home = temp_home(CONFIG);
    std::fs::write(home.library().join("review_queue.json"), "[{").unwrap();
    assert!(library::load_queue(&home).is_err());
}

#[test]
fn schedule_slots_skip_taken_and_off_days() {
    let home = temp_home(CONFIG);
    let cfg = Config::load(&home).unwrap();
    // 2026-10-12 is a Monday.
    let from = parse_when("2026-10-12T10:00").unwrap();
    let mut taken = serde_json::Map::new();
    taken.insert("status".into(), json!("scheduled"));
    taken.insert("scheduled_at".into(), json!("2026-10-12T18:00:00"));
    let slots = next_slots(&cfg, &[taken], 3, from);
    let got: Vec<String> = slots.iter().map(|d| d.format("%a %H:%M").to_string()).collect();
    assert_eq!(got, vec!["Wed 09:00", "Wed 18:00", "Mon 09:00"]);
    assert_eq!(parse_when("2026-10-12").unwrap().format("%H:%M").to_string(), "09:00");
    assert!(parse_when("tomorrow").is_none());
    assert_eq!(month_grid(2026, 10).len(), 42);
    assert!(month_grid(2026, 13).is_empty());
    assert_eq!(shift_month(2026, 1, -1), (2025, 12));
    assert_eq!(shift_month(2026, 12, 1), (2027, 1));
}

#[test]
fn feed_parsing_rss_and_atom() {
    let rss = r#"<?xml version="1.0"?><rss><channel><item><title>Big &amp; news</title><link>https://a/1</link>
        <description>&lt;p&gt;Hello &lt;img src="https://a/i.jpg"&gt; world&lt;/p&gt;</description><pubDate>Mon, 12 Oct 2026</pubDate></item></channel></rss>"#;
    let s = crate::feeds::parse(rss, "https://www.a.com/feed").unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].title, "Big & news");
    assert_eq!(s[0].url, "https://a/1");
    assert_eq!(s[0].summary, "Hello world");
    assert_eq!(s[0].image, "https://a/i.jpg");
    assert_eq!(s[0].source, "a.com");
    let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:media="http://search.yahoo.com/mrss/"><entry><title>Vid</title>
        <link rel="alternate" href="https://yt/v"/><media:group><media:thumbnail url="https://yt/t.jpg"/><media:description>Desc</media:description></media:group></entry></feed>"#;
    let s = crate::feeds::parse(atom, "https://yt").unwrap();
    assert_eq!(s[0].url, "https://yt/v");
    assert_eq!(s[0].image, "https://yt/t.jpg");
    assert_eq!(s[0].summary, "Desc");
    assert!(crate::feeds::parse("not xml", "u").is_err());
}

#[test]
fn copy_merge_only_touches_copy() {
    let base = json!({"title_card": {"headline": "A"}, "hashtags": ["x"], "slug": "s", "content_slides": [{"heading": "H", "image_query": "q"}]});
    let edit = json!({"title_card": {"headline": "B"}, "hashtags": ["evil"], "slug": "t", "content_slides": [{"heading": "H2", "image_query": "changed"}, {"heading": "extra"}]});
    let m = crate::planner::merge_copy(&base, &edit);
    assert_eq!(m["title_card"]["headline"], "B");
    assert_eq!(m["hashtags"], json!(["x"]));
    assert_eq!(m["slug"], "s");
    assert_eq!(m["content_slides"], json!([{"heading": "H2", "image_query": "q"}]));
}

#[test]
fn publish_without_setup_keeps_the_slot() {
    let home = temp_home(CONFIG);
    let cfg = Config::load(&home).unwrap();
    let env = crate::home::Env::default();
    std::fs::create_dir_all(home.outputs().join("k2/p")).unwrap();
    std::fs::write(home.outputs().join("k2/p/01.png"), b"\x89PNG").unwrap();
    let mut e: library::Entry = serde_json::from_value(json!({"id": "1", "brand": "k2", "rel": "k2/p", "files": ["01.png"], "status": "scheduled", "format": "square"})).unwrap();
    let r = crate::meta::publish_entry(&home, &cfg, &env, &mut e);
    assert_eq!(r["sent"], false);
    assert!(r["reason"].as_str().unwrap().contains("META_ACCESS_TOKEN"));
    assert_eq!(library::s(&e, "status"), "scheduled");
    assert!(e.contains_key("blocked_reason"));
    assert_eq!(e["publish_attempts"], 1);
    // Unmapped brand.
    e.insert("brand".into(), json!("nobody"));
    let r = crate::meta::publish_entry_record(&home, &cfg, &env, &e);
    assert!(r["reason"].as_str().unwrap().contains("No Meta account"));
}

/// Reads a real K2 Press folder: `K2_PRESS_HOME=… cargo test -p photocraft-studio real_home -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_home() {
    let Ok(dir) = std::env::var("K2_PRESS_HOME") else { return };
    let home = Home::new(dir).unwrap();
    let cfg = Config::load(&home).unwrap();
    for b in &cfg.brands {
        eprintln!("brand {} ({}) logo={:?} feeds={} types={}", b.key, b.name, b.logo, b.feed_categories().len(), b.content_types.len());
    }
    let q = library::load_queue(&home).unwrap();
    eprintln!("queue: {} entries; backend: {}", q.len(), Backend::resolve(&cfg, &home.env(), &Settings::default()).name());
    eprintln!("next slots: {:?}", next_slots(&cfg, &q, 3, crate::schedule::now()));
}
