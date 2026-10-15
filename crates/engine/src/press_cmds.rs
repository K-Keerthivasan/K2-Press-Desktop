//! Post Studio (`press.*`): build social posts as ordinary documents. A post plan and a brand
//! compile (in `photocraft-press`) to one [`SlideSpec`] per slide; `press.build` opens a new
//! document for each and runs its steps with the same commands a person would use, so every
//! headline, panel and photo stays an editable layer.

use photocraft_press::layout::Fit;
use photocraft_press::{Brand, Format, Plan, SlideSpec, Step, Style};
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

/// Largest canvas side a slide may ask for (posts are at most a few thousand pixels).
const MAX_SIDE: u32 = 8192;
/// Steps per slide; a spec longer than this is refused rather than run.
const MAX_STEPS: usize = 400;

fn plan_param(p: &Value, cmd: &str) -> Result<Plan> {
    let v = p.get("plan").ok_or_else(|| bad(cmd, "pass `plan` (a post plan object)"))?;
    if !v.is_object() {
        return Err(bad(cmd, "`plan` must be an object"));
    }
    Plan::from_json(v).map_err(|e| bad(cmd, e))
}

fn brand_param(p: &Value) -> Brand {
    match p.get("brand") {
        Some(v @ Value::Object(_)) => {
            let mut b = Brand::from_value(p.get("brandKey").and_then(Value::as_str).unwrap_or("brand"), v);
            // The resolved logo path travels as `logo` (config's `logo_path` is relative).
            if let Some(l) = v.get("logo").and_then(Value::as_str).filter(|l| !l.is_empty()) {
                b.logo = Some(l.to_string());
            }
            b
        }
        _ => Brand::default(),
    }
}

fn format_param(p: &Value) -> Option<Format> {
    p.get("format").and_then(Value::as_str).filter(|s| !s.is_empty()).map(Format::from_key)
}

fn style_param(p: &Value, cmd: &str) -> Result<Option<Style>> {
    match p.get("style").and_then(Value::as_str) {
        None | Some("") | Some("auto") => Ok(None),
        Some(k) => Style::from_key(k).map(Some).ok_or_else(|| bad(cmd, format!("unknown style `{k}` (classic | imageForward)"))),
    }
}

fn specs_for(p: &Value, cmd: &str) -> Result<Vec<SlideSpec>> {
    let plan = plan_param(p, cmd)?;
    let brand = brand_param(p);
    Ok(photocraft_press::slides(&plan, &brand, format_param(p), style_param(p, cmd)?))
}

fn slides_query(_s: &mut Session, p: &Value) -> Result<Value> {
    let specs = specs_for(p, "press.slides")?;
    Ok(json!({"slides": specs}))
}

fn f(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// Place an image into `rect`, cover- or contain-fitted; a cover photo is clipped to a frame.
#[allow(clippy::too_many_arguments)]
fn place_image(s: &mut Session, path: &str, rect: [f64; 4], fit: Fit, name: &str, opacity: f64, radius: f64, canvas: (u32, u32)) -> Result<()> {
    let [x, y, w, h] = rect.map(f);
    if w < 1.0 || h < 1.0 {
        return Err(EngineError::Other(format!("image frame for {name} is empty")));
    }
    let full = x <= 0.0 && y <= 0.0 && x + w >= f64::from(canvas.0) && y + h >= f64::from(canvas.1);
    let framed = fit == Fit::Cover && !full;
    if framed {
        let frame = if radius > 0.0 {
            json!({"kind": "roundedRect", "rect": [x, y, w, h], "radii": radius, "fill": "#000000", "name": format!("{name} frame")})
        } else {
            json!({"kind": "rect", "rect": [x, y, w, h], "fill": "#000000", "name": format!("{name} frame")})
        };
        s.execute("shape.create", frame)?;
    }
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let placed = s.execute("file.placeEmbedded", json!({"path": path, "fit": false, "scale": 100, "smartObject": true, "center": [cx, cy]}));
    let placed = match placed {
        Ok(v) => v,
        Err(e) => {
            if framed {
                // Leave no empty black frame behind.
                let _ = s.execute("layer.delete", json!({}));
            }
            return Err(e);
        }
    };
    let b: Vec<f64> = placed.get("bounds").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    if let [x0, y0, x1, y1] = b[..] {
        let (bw, bh) = (x1 - x0, y1 - y0);
        if bw > 0.5 && bh > 0.5 {
            let k = match fit {
                Fit::Cover => (w / bw).max(h / bh),
                Fit::Contain => (w / bw).min(h / bh),
            };
            let dx = cx - (x0 + x1) / 2.0;
            let dy = cy - (y0 + y1) / 2.0;
            if (k - 1.0).abs() > 1e-3 || dx.abs() > 0.5 || dy.abs() > 0.5 {
                s.execute("edit.transform", json!({"scaleX": k * 100.0, "scaleY": k * 100.0, "dx": dx, "dy": dy}))?;
            }
        }
    }
    let mut props = json!({"name": name, "opacity": opacity.clamp(0.0, 1.0)});
    if framed {
        props["clipped"] = json!(true);
    }
    s.execute("layer.setProps", props)?;
    Ok(())
}

/// Open a new document for `spec` and draw it. Steps that fail (a missing photo, say) are skipped
/// and reported, so one bad image never loses the rest of the slide.
pub fn build_spec(s: &mut Session, spec: &SlideSpec, title: &str) -> Result<Value> {
    if spec.width == 0 || spec.height == 0 || spec.width > MAX_SIDE || spec.height > MAX_SIDE {
        return Err(bad("press.build", format!("slide size {}×{} is outside 1–{MAX_SIDE}", spec.width, spec.height)));
    }
    if spec.steps.len() > MAX_STEPS {
        return Err(bad("press.build", format!("a slide may have at most {MAX_STEPS} steps")));
    }
    let name = if title.is_empty() { spec.name.clone() } else { format!("{title} · {}", spec.name) };
    s.execute("file.new", json!({"width": spec.width, "height": spec.height, "background": photocraft_press::layout::color(&spec.background, "#000000"), "name": name, "resolution": 72}))?;
    let index = s.active_index().ok_or(EngineError::NoDocument)?;
    let mut warnings: Vec<String> = Vec::new();
    for step in &spec.steps {
        let r = match step {
            Step::Cmd { id, params } => {
                if id.starts_with("press.") || id.starts_with("file.") {
                    Err(EngineError::Other(format!("`{id}` is not allowed inside a slide")))
                } else {
                    s.execute(id, params.clone()).map(|_| ())
                }
            }
            Step::Image { path, rect, fit, name, opacity, radius } => place_image(s, path, *rect, *fit, name, *opacity, *radius, (spec.width, spec.height)),
        };
        if let Err(e) = r {
            warnings.push(e.to_string());
        }
        // A step must never move the build to another document.
        if s.active_index() != Some(index) {
            s.set_active(index);
        }
    }
    // Start with the top layer selected, as after building by hand.
    Ok(json!({"index": index, "name": name, "slide": spec.name, "kind": spec.kind, "warnings": warnings}))
}

fn build(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "press.build";
    let plan = plan_param(p, CMD)?;
    let specs = specs_for(p, CMD)?;
    let only: Option<Vec<usize>> = match p.get("slides") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => Some(a.iter().map(|v| v.as_u64().map(|n| n as usize).ok_or_else(|| bad(CMD, "`slides` holds slide indices (0 = first)"))).collect::<Result<_>>()?),
        Some(_) => return Err(bad(CMD, "`slides` must be a list of slide indices")),
    };
    if let Some(o) = &only
        && let Some(i) = o.iter().find(|i| **i >= specs.len())
    {
        return Err(bad(CMD, format!("no slide {i} (the post has {})", specs.len())));
    }
    let title = crate::press_cmds::short_title(plan.title());
    let mut docs = Vec::new();
    for (i, spec) in specs.iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.contains(&i)) {
            continue;
        }
        docs.push(build_spec(s, spec, &title)?);
    }
    if let Some(first) = docs.first().and_then(|d| d.get("index")).and_then(Value::as_u64) {
        s.set_active(first as usize);
    }
    Ok(json!({"documents": docs, "slides": specs.len()}))
}

fn build_one(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "press.buildSpec";
    let v = p.get("spec").ok_or_else(|| bad(CMD, "pass `spec` (a slide spec from press.slides)"))?;
    let spec: SlideSpec = serde_json::from_value(v.clone()).map_err(|e| bad(CMD, format!("not a slide spec: {e}")))?;
    build_spec(s, &spec, p.get("title").and_then(Value::as_str).unwrap_or(""))
}

/// Document-name prefix: the post title, at most 40 characters.
pub fn short_title(t: &str) -> String {
    let t = t.trim();
    if t.chars().count() <= 40 { t.to_string() } else { t.chars().take(39).collect::<String>().trim_end().to_string() + "…" }
}

fn formats(_s: &mut Session, _p: &Value) -> Result<Value> {
    let formats: Vec<Value> = Format::ALL
        .iter()
        .map(|f| {
            let (w, h) = f.size();
            json!({"key": f.key(), "label": f.label(), "width": w, "height": h})
        })
        .collect();
    let styles: Vec<Value> = [Style::Classic, Style::ImageForward].iter().map(|s| json!({"key": s.key(), "label": s.label()})).collect();
    Ok(json!({"formats": formats, "styles": styles}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "press.build",
            label: "Build Post",
            menu: &[],
            shortcut: None,
            params: r##"{"plan":{post plan: title_card, content_slides, outro_card, caption, hashtags… or headline/body for single formats},"brand":{name, handle, tagline, theme:{navy,navy2,accent,accent2,text}, logo:path, image_forward, font…}?,"format":"carousel|square|story|x|quote|cover"?=plan's,"style":"classic|imageForward"?=brand's,"slides":[index,…]? (default all)} → {documents:[{index,name,slide,kind,warnings}],slides}"##,
            enabled: crate::commands::always,
            run: build,
            journal: true,
        },
        CommandSpec {
            id: "press.slides",
            label: "Post Layout",
            menu: &[],
            shortcut: None,
            params: r##"{"plan":{…},"brand":{…}?,"format":str?,"style":str?} → {slides:[{name,kind,width,height,background,steps:[{step:"cmd",id,params}|{step:"image",path,rect,fit,name,opacity,radius}]}]}"##,
            enabled: crate::commands::always,
            run: slides_query,
            journal: false,
        },
        CommandSpec {
            id: "press.buildSpec",
            label: "Build Slide",
            menu: &[],
            shortcut: None,
            params: r##"{"spec":{a slide from press.slides},"title":str?} → {index,name,slide,kind,warnings}"##,
            enabled: crate::commands::always,
            run: build_one,
            journal: true,
        },
        CommandSpec {
            id: "press.formats",
            label: "Post Formats",
            menu: &[],
            shortcut: None,
            params: r##"{} → {formats:[{key,label,width,height}],styles:[{key,label}]}"##,
            enabled: crate::commands::always,
            run: formats,
            journal: false,
        },
    ]
}

/// Whether a document is open (for callers that build into the active document's session).
pub fn ready(s: &Session) -> bool {
    has_doc(s).is_ok()
}
