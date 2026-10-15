//! Post Studio (Window › Post Studio): K2 Press built into PhotoCraft. A panel left of the canvas
//! that makes social posts from an idea, RSS stories or a spreadsheet, writes them with local AI,
//! builds every slide as an editable PhotoCraft document (`press.build`), exports the edited slides
//! to the K2 Press library, and runs the review queue, the calendar and Instagram / Facebook
//! publishing. It reads the K2 Press folder in place (see `photocraft_studio::home`), so the earlier
//! web app keeps working alongside.
//!
//! Slow work (AI, network, publishing) runs on a worker thread; its results come back as closures
//! applied on the UI thread, which owns the session.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Color32, RichText};
use photocraft_doc::DocId;
use photocraft_press::csv::CsvPost;
use photocraft_press::{ContentSlide, Format, Plan, Style};
use photocraft_studio::config::Config;
use photocraft_studio::home::{Home, Settings};
use photocraft_studio::library::{self, Entry};
use photocraft_studio::llm::Backend;
use photocraft_studio::planner::{self, Request};
use photocraft_studio::{Story, feeds, images, meta, schedule};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

type Apply = Box<dyn FnOnce(&mut PhotocraftApp) + Send>;

enum Msg {
    Progress(String),
    Apply(Apply),
    Done,
}

struct Job {
    label: String,
    progress: String,
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

/// A handle the worker uses to report back.
#[derive(Clone)]
struct Reply {
    tx: Sender<Msg>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl Reply {
    fn progress(&self, s: impl Into<String>) {
        let _ = self.tx.send(Msg::Progress(s.into()));
        self.ctx.request_repaint();
    }
    fn apply(&self, f: impl FnOnce(&mut PhotocraftApp) + Send + 'static) {
        let _ = self.tx.send(Msg::Apply(Box::new(f)));
        self.ctx.request_repaint();
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Create,
    Post,
    Queue,
    Calendar,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Source {
    #[default]
    Idea,
    Stories,
    Sheet,
    Saved,
}

/// Everything the panel remembers between frames.
pub struct State {
    loaded: bool,
    settings: Settings,
    home: Option<Home>,
    cfg: Option<Config>,
    load_error: String,
    brand: String,
    tab: Tab,
    // Create
    source: Source,
    idea: String,
    notes: String,
    total_slides: usize,
    platform: String,
    content_type: String,
    tone: String,
    polish: bool,
    format: Format,
    angles: Vec<String>,
    category: String,
    stories: Vec<Story>,
    picked: Vec<bool>,
    fetch_n: usize,
    csv_name: String,
    csv_posts: Vec<CsvPost>,
    csv_ai: bool,
    batch_keep_open: bool,
    saved: Vec<(PathBuf, String)>,
    // Post
    plan: Option<Plan>,
    style: Option<Style>,
    image_source: String,
    image_links: BTreeMap<usize, String>,
    built: Vec<(DocId, String)>,
    // Queue / calendar
    queue: Vec<Entry>,
    queue_filter: String,
    when: HashMap<String, String>,
    confirm_publish: Option<String>,
    thumbs: HashMap<PathBuf, Option<egui::TextureHandle>>,
    cal: (i32, u32),
    // Settings
    home_input: String,
    verify_text: String,
    // Jobs
    job: Option<Job>,
    status: String,
    error: String,
    last_tick: Option<Instant>,
}

impl Default for State {
    fn default() -> Self {
        let now = schedule::now();
        use photocraft_studio::chrono::Datelike;
        State {
            loaded: false,
            settings: Settings::default(),
            home: None,
            cfg: None,
            load_error: String::new(),
            brand: String::new(),
            tab: Tab::Create,
            source: Source::Idea,
            idea: String::new(),
            notes: String::new(),
            total_slides: 5,
            platform: "instagram".into(),
            content_type: "general".into(),
            tone: String::new(),
            polish: false,
            format: Format::Carousel,
            angles: Vec::new(),
            category: String::new(),
            stories: Vec::new(),
            picked: Vec::new(),
            fetch_n: 30,
            csv_name: String::new(),
            csv_posts: Vec::new(),
            csv_ai: false,
            batch_keep_open: false,
            saved: Vec::new(),
            plan: None,
            style: None,
            image_source: "pexels".into(),
            image_links: BTreeMap::new(),
            built: Vec::new(),
            queue: Vec::new(),
            queue_filter: String::new(),
            when: HashMap::new(),
            confirm_publish: None,
            thumbs: HashMap::new(),
            cal: (now.year(), now.month()),
            home_input: String::new(),
            verify_text: String::new(),
            job: None,
            status: String::new(),
            error: String::new(),
            last_tick: None,
        }
    }
}

impl State {
    fn reload(&mut self) {
        self.loaded = true;
        self.settings = Settings::load();
        self.load_error.clear();
        match Home::discover(&self.settings) {
            Ok(h) => {
                self.home_input = h.root.to_string_lossy().to_string();
                match Config::load(&h) {
                    Ok(c) => {
                        let pick = [self.brand.clone(), self.settings.brand.clone(), c.active_brand.clone()].into_iter().find(|k| c.brand(k).is_some());
                        self.brand = pick.or_else(|| c.brands.first().map(|b| b.key.clone())).unwrap_or_default();
                        self.cfg = Some(c);
                    }
                    Err(e) => {
                        self.load_error = e.0;
                        self.cfg = None;
                    }
                }
                self.home = Some(h);
                self.refresh_queue();
            }
            Err(e) => {
                self.load_error = e.0;
                self.home = None;
                self.cfg = None;
            }
        }
    }

    fn brand(&self) -> Option<photocraft_press::Brand> {
        self.cfg.as_ref()?.brand(&self.brand).cloned()
    }

    fn refresh_queue(&mut self) {
        let Some(h) = &self.home else { return };
        match library::load_queue(h) {
            Ok(mut q) => {
                q.sort_by(|a, b| library::s(b, "created").cmp(library::s(a, "created")));
                self.queue = q;
            }
            Err(e) => self.error = e.0,
        }
    }

    fn backend(&self) -> Option<Backend> {
        Some(Backend::resolve(self.cfg.as_ref()?, &self.home.as_ref()?.env(), &self.settings))
    }

    fn busy(&self) -> bool {
        self.job.is_some()
    }
}

/// Start a worker. `run` gets a [`Reply`] to report progress and hand back results.
fn spawn(app: &mut PhotocraftApp, ctx: &egui::Context, label: &str, run: impl FnOnce(Reply) + Send + 'static) {
    if app.press.busy() {
        app.press.error = "Wait for the current task to finish (or stop it).".into();
        return;
    }
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let reply = Reply { tx: tx.clone(), cancel: cancel.clone(), ctx: ctx.clone() };
    app.press.error.clear();
    app.press.status.clear();
    app.press.job = Some(Job { label: label.into(), progress: String::new(), rx, cancel, started: Instant::now() });
    let spawned = std::thread::Builder::new().name("post-studio".into()).spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(reply.clone())));
        if r.is_err() {
            reply.apply(|app| app.press.error = "The task stopped unexpectedly.".into());
        }
        let _ = tx.send(Msg::Done);
        reply.ctx.request_repaint();
    });
    if let Err(e) = spawned {
        app.press.job = None;
        app.press.error = format!("could not start a worker: {e}");
    }
}

fn poll(app: &mut PhotocraftApp) {
    let mut applies: Vec<Apply> = Vec::new();
    let mut done = false;
    if let Some(job) = &mut app.press.job {
        while let Ok(m) = job.rx.try_recv() {
            match m {
                Msg::Progress(p) => job.progress = p,
                Msg::Apply(f) => applies.push(f),
                Msg::Done => done = true,
            }
        }
    }
    for f in applies {
        f(app);
    }
    if done {
        app.press.job = None;
    }
}

// ── Building and exporting ───────────────────────────────────────────────────

fn brand_json(app: &PhotocraftApp, key: &str) -> Value {
    app.press.cfg.as_ref().and_then(|c| c.brand(key)).map(Config::brand_json).unwrap_or(Value::Null)
}

/// Built slide documents: (document, slide name).
type Built = Vec<(DocId, String)>;

fn doc_index(app: &PhotocraftApp, id: DocId) -> Option<usize> {
    app.session.documents().iter().position(|d| d.doc.id == id)
}

/// Build `plan` into documents; returns (DocId, slide name) per slide and any warnings.
fn build_docs(app: &mut PhotocraftApp, plan: &Plan, brand: &str, style: Option<Style>) -> Result<(Built, Vec<String>), String> {
    let mut p = json!({"plan": plan.to_json(), "brand": brand_json(app, brand), "brandKey": brand, "format": plan.format().key()});
    if let Some(s) = style {
        p["style"] = json!(s.key());
    }
    let r = app.session.execute("press.build", p).map_err(|e| {
        app.sync_views();
        e.to_string()
    });
    let r = r?;
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for d in r.get("documents").and_then(Value::as_array).into_iter().flatten() {
        let Some(i) = d.get("index").and_then(Value::as_u64) else { continue };
        let Some(doc) = app.session.documents().get(i as usize) else { continue };
        out.push((doc.doc.id, d.get("slide").and_then(Value::as_str).unwrap_or("slide").to_string()));
        warnings.extend(d.get("warnings").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
    }
    // One view per document: the shell indexes views by document.
    app.sync_views();
    Ok((out, warnings))
}

/// Export built documents (as they are now, with any edits) into a new library post.
fn export_docs(app: &mut PhotocraftApp, plan: &Plan, brand: &str, docs: &[(DocId, String)]) -> Result<Entry, String> {
    let home = app.press.home.clone().ok_or("set the K2 Press folder first (Settings)")?;
    if docs.is_empty() {
        return Err("nothing to export: build the post first".into());
    }
    let (rel, dir) = library::new_output_dir(&home, brand, if plan.slug.is_empty() { plan.title() } else { &plan.slug }, plan.format().key()).map_err(|e| e.0)?;
    let before = app.session.active_index();
    let mut files = Vec::new();
    for (id, name) in docs {
        let i = doc_index(app, *id).ok_or_else(|| format!("slide {name} was closed; rebuild the post"))?;
        app.session.set_active(i);
        let file = format!("{name}.png");
        let path = dir.join(&file).to_string_lossy().replace('\\', "/");
        app.session.execute("file.saveACopy", json!({"path": path})).map_err(|e| format!("{name}: {e}"))?;
        files.push(file);
    }
    if let Some(b) = before {
        app.session.set_active(b);
    }
    library::add_post(&home, brand, plan, &rel, &files).map_err(|e| e.0)
}

fn close_docs(app: &mut PhotocraftApp, docs: &[(DocId, String)]) {
    for (id, _) in docs {
        if let Some(i) = doc_index(app, *id) {
            app.session.close(i);
        }
    }
    app.sync_views();
}

/// Render a whole post headlessly (batch): build, export, close (unless kept open).
fn render_post(app: &mut PhotocraftApp, plan: &Plan, brand: &str, keep_open: bool) -> Result<Entry, String> {
    let style = app.press.style;
    let (docs, _warnings) = build_docs(app, plan, brand, style)?;
    let r = export_docs(app, plan, brand, &docs);
    if !keep_open {
        close_docs(app, &docs);
    }
    r
}

fn open_plan(app: &mut PhotocraftApp, mut plan: Plan) {
    if let Some(b) = app.press.brand() {
        plan.normalize(&b);
    }
    app.press.format = plan.format();
    app.press.plan = Some(plan);
    app.press.built.clear();
    app.press.image_links.clear();
    app.press.tab = Tab::Post;
}

// ── Panel ────────────────────────────────────────────────────────────────────

pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if !app.press.loaded {
        app.press.reload();
    }
    poll(app);
    scheduler_tick(app, ui.ctx());
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("post-studio")
        .resizable(true)
        .default_size(400.0)
        .min_size(320.0)
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 8)))
        .show(ui, |ui| {
            header(app, ui);
            ui.separator();
            if app.press.home.is_none() || app.press.cfg.is_none() {
                app.press.tab = Tab::Settings;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match app.press.tab {
                Tab::Create => create_tab(app, ui),
                Tab::Post => post_tab(app, ui),
                Tab::Queue => queue_tab(app, ui),
                Tab::Calendar => calendar_tab(app, ui),
                Tab::Settings => settings_tab(app, ui),
            });
        });
}

fn header(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Post Studio").strong().size(15.0));
        if let Some(cfg) = &app.press.cfg {
            let current = cfg.brand(&app.press.brand).map(|b| b.name.clone()).unwrap_or_else(|| "—".into());
            let mut pick = None;
            egui::ComboBox::from_id_salt("press-brand").selected_text(current).show_ui(ui, |ui| {
                for b in &cfg.brands {
                    if ui.selectable_label(b.key == app.press.brand, &b.name).clicked() {
                        pick = Some(b.key.clone());
                    }
                }
            });
            if let Some(k) = pick {
                app.press.brand = k.clone();
                app.press.settings.brand = k;
                let _ = app.press.settings.save();
                app.press.stories.clear();
                app.press.saved.clear();
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let pending = app.press.queue.iter().filter(|e| library::s(e, "status") == "pending").count();
        for (tab, label) in [(Tab::Create, "Create".to_string()), (Tab::Post, "Post".into()), (Tab::Queue, if pending > 0 { format!("Queue ({pending})") } else { "Queue".into() }), (Tab::Calendar, "Calendar".into()), (Tab::Settings, "Settings".into())] {
            if ui.selectable_label(app.press.tab == tab, label).clicked() {
                app.press.tab = tab;
                if matches!(tab, Tab::Queue | Tab::Calendar) {
                    app.press.refresh_queue();
                }
            }
        }
    });
    if let Some(job) = &app.press.job {
        let mut stop = false;
        ui.horizontal(|ui| {
            ui.spinner();
            let p = if job.progress.is_empty() { String::new() } else { format!(" — {}", job.progress) };
            ui.label(format!("{}{p} ({}s)", job.label, job.started.elapsed().as_secs()));
            stop = ui.small_button("Stop").clicked();
        });
        if stop {
            job.cancel.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }
    if !app.press.error.is_empty() {
        let mut clear = false;
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(Color32::from_rgb(235, 90, 80), &app.press.error);
            clear = ui.small_button("×").clicked();
        });
        if clear {
            app.press.error.clear();
        }
    } else if !app.press.status.is_empty() {
        ui.colored_label(Color32::from_rgb(90, 190, 120), &app.press.status);
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
}

fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[(&str, &str)]) {
    let label = options.iter().find(|o| o.0 == value.as_str()).map_or(value.as_str(), |o| o.1).to_string();
    egui::ComboBox::from_id_salt(id).selected_text(label).width(200.0).show_ui(ui, |ui| {
        for (k, l) in options {
            ui.selectable_value(value, (*k).to_string(), *l);
        }
    });
}

fn format_combo(ui: &mut egui::Ui, id: &str, f: &mut Format) {
    egui::ComboBox::from_id_salt(id).selected_text(f.label()).width(200.0).show_ui(ui, |ui| {
        for x in Format::ALL {
            ui.selectable_value(f, x, x.label());
        }
    });
}

// ── Create ───────────────────────────────────────────────────────────────────

fn create_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (s, l) in [(Source::Idea, "Idea"), (Source::Stories, "Stories"), (Source::Sheet, "Sheet"), (Source::Saved, "Saved")] {
            ui.selectable_value(&mut app.press.source, s, l);
        }
    });
    ui.separator();
    match app.press.source {
        Source::Idea => idea_ui(app, ui),
        Source::Stories => stories_ui(app, ui),
        Source::Sheet => sheet_ui(app, ui),
        Source::Saved => saved_ui(app, ui),
    }
}

fn options_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let s = &mut app.press;
    egui::Grid::new("press-opts").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Format");
        format_combo(ui, "press-fmt", &mut s.format);
        ui.end_row();
        if s.format == Format::Carousel {
            ui.label("Slides");
            ui.add(egui::Slider::new(&mut s.total_slides, 3..=10));
            ui.end_row();
        }
        ui.label("Platform");
        let plats: Vec<(&str, &str)> = planner::PLATFORMS.iter().map(|p| (p.0, p.1)).collect();
        combo(ui, "press-plat", &mut s.platform, &plats);
        ui.end_row();
        ui.label("Content type");
        let brand_types = s.cfg.as_ref().and_then(|c| c.brand(&s.brand)).map(|b| b.content_types.clone()).unwrap_or_default();
        let mut types: Vec<(String, String)> = vec![("general".into(), "General".into())];
        if brand_types.is_empty() {
            types.extend(planner::CONTENT_TYPES.iter().skip(1).take(6).map(|c| (c.0.to_string(), c.1.to_string())));
        } else {
            types.extend(brand_types.iter().map(|c| (c.value.clone(), c.label.clone())));
        }
        let refs: Vec<(&str, &str)> = types.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        combo(ui, "press-ctype", &mut s.content_type, &refs);
        ui.end_row();
        ui.label("Tone");
        ui.add(egui::TextEdit::singleline(&mut s.tone).hint_text("optional: positive, critical…").desired_width(200.0));
        ui.end_row();
    });
    ui.checkbox(&mut s.polish, "Brand-voice polish pass (slower)");
}

fn request(app: &PhotocraftApp, story: Story, manual: bool) -> Request {
    let s = &app.press;
    Request { story, manual, total_slides: Some(s.total_slides), platform: s.platform.clone(), content_type: s.content_type.clone(), tone: s.tone.clone(), polish: s.polish }
}

fn plan_with_ai(ai: &Backend, brand: &photocraft_press::Brand, req: &Request, fmt: Format) -> photocraft_studio::Result<Plan> {
    match fmt {
        Format::Carousel => planner::plan_carousel(ai, brand, req),
        Format::Cover => {
            let mut p = Plan::blank(brand, 1);
            p.format = "cover".into();
            p.caption = brand.tagline.clone();
            Ok(p)
        }
        f => planner::plan_single(ai, brand, req, f.key()),
    }
}

fn idea_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    section(ui, "Your idea");
    ui.add(egui::TextEdit::singleline(&mut app.press.idea).hint_text("What is the post about?").desired_width(f32::INFINITY));
    ui.add(egui::TextEdit::multiline(&mut app.press.notes).hint_text("Notes, facts, links — the AI uses only what you write here").desired_rows(5).desired_width(f32::INFINITY));
    options_ui(app, ui);
    let ready = !app.press.idea.trim().is_empty() && !app.press.busy();
    ui.horizontal(|ui| {
        if ui.add_enabled(ready, egui::Button::new("Write post with AI")).clicked() {
            let (Some(ai), Some(brand)) = (app.press.backend(), app.press.brand()) else { return };
            let story = Story { title: app.press.idea.trim().into(), summary: if app.press.notes.trim().is_empty() { app.press.idea.trim().into() } else { app.press.notes.trim().into() }, ..Default::default() };
            let req = request(app, story, true);
            let fmt = app.press.format;
            spawn(app, &ctx, &format!("Writing with {}", ai.name()), move |r| {
                let res = plan_with_ai(&ai, &brand, &req, fmt);
                r.apply(move |app| match res {
                    Ok(p) => {
                        open_plan(app, p);
                        app.press.status = "Plan ready — fetch images, then Build.".into();
                    }
                    Err(e) => app.press.error = e.0,
                });
            });
        }
        if ui.add_enabled(ready, egui::Button::new("Suggest angles")).clicked() {
            let (Some(ai), Some(brand)) = (app.press.backend(), app.press.brand()) else { return };
            let (idea, notes) = (app.press.idea.clone(), app.press.notes.clone());
            spawn(app, &ctx, "Finding angles", move |r| {
                let res = planner::suggest_angles(&ai, &brand, &idea, &notes);
                r.apply(move |app| match res {
                    Ok(a) => app.press.angles = a,
                    Err(e) => app.press.error = e.0,
                });
            });
        }
    });
    if ui.add_enabled(!app.press.busy(), egui::Button::new("Start blank (write it yourself)")).clicked()
        && let Some(b) = app.press.brand()
    {
        let mut p = Plan::blank(&b, app.press.total_slides.saturating_sub(2));
        p.format = app.press.format.key().into();
        if !app.press.idea.trim().is_empty() {
            p.title_card.headline = app.press.idea.trim().into();
            p.headline = app.press.idea.trim().into();
        }
        open_plan(app, p);
    }
    if !app.press.angles.is_empty() {
        section(ui, "Angles (click to use)");
        let mut use_angle = None;
        for a in &app.press.angles {
            if ui.link(format!("• {a}")).clicked() {
                use_angle = Some(a.clone());
            }
        }
        if let Some(a) = use_angle {
            app.press.idea = a;
            app.press.angles.clear();
        }
    }
}

fn stories_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let Some(brand) = app.press.brand() else {
        ui.label("Pick a brand.");
        return;
    };
    let cats = brand.feed_categories();
    if cats.is_empty() {
        ui.label("This brand has no feeds. Add them under brands ▸ feeds ▸ categories in config.yaml.");
        return;
    }
    if !cats.iter().any(|c| c.0 == app.press.category) {
        app.press.category = cats.iter().find(|c| c.2).map(|c| c.0.clone()).unwrap_or_default();
    }
    ui.horizontal(|ui| {
        ui.label("Category");
        let cur = cats.iter().find(|c| c.0 == app.press.category).map(|c| c.1.clone()).unwrap_or_default();
        egui::ComboBox::from_id_salt("press-cat").selected_text(cur).show_ui(ui, |ui| {
            for c in &cats {
                ui.selectable_value(&mut app.press.category, c.0.clone(), format!("{} ({} feeds)", c.1, c.3.len()));
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label("Score up to");
        ui.add(egui::DragValue::new(&mut app.press.fetch_n).range(5..=100));
        ui.label("stories");
    });
    ui.horizontal(|ui| {
        if ui.add_enabled(!app.press.busy(), egui::Button::new("Fetch & score")).clicked() {
            let urls = cats.iter().find(|c| c.0 == app.press.category).map(|c| c.3.clone()).unwrap_or_default();
            let (Some(home), Some(ai)) = (app.press.home.clone(), app.press.backend()) else { return };
            let n = app.press.fetch_n;
            let b = brand.clone();
            spawn(app, &ctx, "Fetching stories", move |r| {
                let seen = library::used_urls(&home);
                let (mut stories, errors) = feeds::fetch_all(&urls, &seen);
                stories.truncate(n);
                r.progress(format!("scoring {} stories with {}", stories.len(), ai.name()));
                let c = r.clone();
                let scored = planner::score(&ai, &b, stories.clone(), &move || c.cancelled());
                r.apply(move |app| {
                    let list = match scored {
                        Ok(s) => s,
                        Err(e) => {
                            app.press.error = format!("Scoring failed ({}); showing unscored stories.", e.0);
                            stories
                        }
                    };
                    app.press.picked = vec![false; list.len()];
                    app.press.stories = list;
                    if !errors.is_empty() {
                        app.press.error = format!("{} feed(s) failed: {}", errors.len(), errors.join("; "));
                    }
                });
            });
        }
        if !app.press.stories.is_empty() && ui.button("Clear").clicked() {
            app.press.stories.clear();
        }
    });
    if app.press.stories.is_empty() {
        return;
    }
    options_ui(app, ui);
    let n_picked = app.press.picked.iter().filter(|p| **p).count();
    ui.horizontal(|ui| {
        if ui.add_enabled(n_picked > 0 && !app.press.busy(), egui::Button::new(format!("Generate {n_picked} selected → queue"))).clicked() {
            let picked: Vec<Story> = app.press.stories.iter().zip(&app.press.picked).filter(|(_, p)| **p).map(|(s, _)| s.clone()).collect();
            let reqs: Vec<Request> = picked.into_iter().map(|s| request(app, s, false)).collect();
            batch(app, &ctx, reqs);
        }
        ui.checkbox(&mut app.press.batch_keep_open, "keep slides open");
    });
    let mut edit = None;
    for (i, s) in app.press.stories.iter().enumerate() {
        ui.separator();
        ui.horizontal(|ui| {
            if let Some(p) = app.press.picked.get_mut(i) {
                ui.checkbox(p, "");
            }
            let score = s.score.map(|v| format!("{v}")).unwrap_or_else(|| "–".into());
            let col = match s.score {
                Some(v) if v >= 70 => Color32::from_rgb(90, 190, 120),
                Some(v) if v >= 45 => Color32::from_rgb(220, 180, 70),
                Some(_) => Color32::from_rgb(200, 100, 90),
                None => Color32::GRAY,
            };
            ui.label(RichText::new(score).strong().color(col));
            ui.vertical(|ui| {
                ui.label(RichText::new(&s.title).strong());
                ui.label(RichText::new(format!("{} · {}", s.source, s.reason)).small().weak());
                ui.horizontal(|ui| {
                    if ui.small_button("Make this post").clicked() {
                        edit = Some(s.clone());
                    }
                    if !s.url.is_empty() {
                        ui.hyperlink_to(RichText::new("source").small(), &s.url);
                    }
                });
            });
        });
    }
    if let Some(story) = edit {
        let (Some(ai), Some(b)) = (app.press.backend(), app.press.brand()) else { return };
        let req = request(app, story, false);
        let fmt = app.press.format;
        spawn(app, &ctx, &format!("Writing with {}", ai.name()), move |r| {
            let res = plan_with_ai(&ai, &b, &req, fmt);
            r.apply(move |app| match res {
                Ok(p) => open_plan(app, p),
                Err(e) => app.press.error = e.0,
            });
        });
    }
}

/// Plan (AI) and render several posts into the queue, one after another.
fn batch(app: &mut PhotocraftApp, ctx: &egui::Context, reqs: Vec<Request>) {
    let (Some(ai), Some(brand), Some(home)) = (app.press.backend(), app.press.brand(), app.press.home.clone()) else { return };
    let (Some(env), fmt, key, source) = (Some(home.env()), app.press.format, app.press.brand.clone(), app.press.image_source.clone()) else { return };
    let n = reqs.len();
    spawn(app, ctx, "Generating posts", move |r| {
        for (i, req) in reqs.into_iter().enumerate() {
            if r.cancelled() {
                break;
            }
            r.progress(format!("{}/{n}: writing “{}”", i + 1, photocraft_press::text::clip(&req.story.title, 40)));
            let mut plan = match plan_with_ai(&ai, &brand, &req, fmt) {
                Ok(p) => p,
                Err(e) => {
                    let msg = format!("{}: {}", req.story.title, e.0);
                    r.apply(move |app| app.press.error = msg);
                    continue;
                }
            };
            r.progress(format!("{}/{n}: images", i + 1));
            fill_images(&home, &env, &source, &mut plan, &req.story.image);
            let k = key.clone();
            r.apply(move |app| {
                let keep = app.press.batch_keep_open;
                match render_post(app, &plan, &k, keep) {
                    Ok(_) => app.press.status = format!("Added {} post(s) to the queue.", i + 1),
                    Err(e) => app.press.error = e,
                }
                app.press.refresh_queue();
            });
        }
    });
}

/// Fetch a picture for every slide that has a query but no image (best effort).
fn fill_images(home: &Home, env: &photocraft_studio::home::Env, source: &str, plan: &mut Plan, story_image: &str) {
    if plan.image.is_empty() && !story_image.is_empty()
        && let Ok(p) = images::download(home, story_image, "feed", &plan.slug)
    {
        plan.image = p.to_string_lossy().replace('\\', "/");
    }
    for s in &mut plan.content_slides {
        if s.image.is_empty() && !s.image_query.trim().is_empty()
            && let Ok(p) = images::fetch_for_query(home, env, source, &s.image_query)
        {
            s.image = p.to_string_lossy().replace('\\', "/");
        }
    }
    if plan.image.is_empty() {
        let q = if plan.image_query.trim().is_empty() { plan.content_slides.first().map(|s| s.image_query.clone()).unwrap_or_default() } else { plan.image_query.clone() };
        if !q.trim().is_empty()
            && let Ok(p) = images::fetch_for_query(home, env, source, &q)
        {
            plan.image = p.to_string_lossy().replace('\\', "/");
        }
    }
}

fn sheet_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.label("One row per post (title, subtitle, slide1_heading, slide1_body, …, cta, caption, hashtags) or one row per slide grouped by post_id.");
    ui.horizontal(|ui| {
        if ui.button("Open sheet (.csv)…").clicked() {
            let r = app.pick_file_bytes_filtered(&["csv", "tsv", "txt"], |app, name, bytes| {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let (h, rows) = photocraft_press::csv::parse(&text)?;
                let posts = photocraft_press::csv::to_posts(&h, &rows);
                app.press.csv_name = std::path::Path::new(&name).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(name);
                app.press.status = format!("{} post(s) found ({} sheet).", posts.len(), photocraft_press::csv::detect_shape(&h));
                app.press.csv_posts = posts;
                Ok(Value::Null)
            });
            if let Err(e) = r
                && e != "cancelled"
            {
                app.press.error = e;
            }
        }
        if ui.button("Save template…").clicked() {
            let r = app.pick_save("post_template.csv", |_app, path| {
                std::fs::write(&path, photocraft_press::csv::TEMPLATE_CSV).map_err(|e| e.to_string())?;
                Ok(Value::Null)
            });
            if let Err(e) = r
                && e != "cancelled"
            {
                app.press.error = e;
            }
        }
    });
    if app.press.csv_posts.is_empty() {
        return;
    }
    section(ui, &format!("{} — {} posts", app.press.csv_name, app.press.csv_posts.len()));
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.press.csv_ai, false, "Use my copy as written");
        ui.radio_value(&mut app.press.csv_ai, true, "AI writes from each row");
    });
    ui.horizontal(|ui| {
        ui.label("Format");
        format_combo(ui, "press-csv-fmt", &mut app.press.format);
    });
    ui.checkbox(&mut app.press.batch_keep_open, "keep slides open");
    let mut open = None;
    for (i, p) in app.press.csv_posts.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}.", i + 1));
            ui.label(RichText::new(&p.title).strong());
            ui.label(RichText::new(format!("{} slides", p.slides.len())).weak());
            if ui.small_button("Edit").clicked() {
                open = Some(i);
            }
        });
    }
    if let Some(i) = open
        && let (Some(p), Some(b)) = (app.press.csv_posts.get(i).cloned(), app.press.brand())
    {
        open_plan(app, photocraft_press::csv::to_plan(&p, &b, app.press.format.key()));
    }
    if ui.add_enabled(!app.press.busy(), egui::Button::new(format!("Generate all {} → queue", app.press.csv_posts.len()))).clicked() {
        let posts = app.press.csv_posts.clone();
        if app.press.csv_ai {
            let reqs = posts
                .iter()
                .map(|p| {
                    let (title, notes) = photocraft_press::csv::to_brief(p);
                    let mut r = request(app, Story { title, summary: notes, ..Default::default() }, true);
                    if !p.tone.is_empty() {
                        r.tone = p.tone.clone();
                    }
                    r
                })
                .collect();
            batch(app, &ctx, reqs);
        } else {
            let (Some(b), Some(home)) = (app.press.brand(), app.press.home.clone()) else { return };
            let (env, fmt, key, source) = (home.env(), app.press.format, app.press.brand.clone(), app.press.image_source.clone());
            let n = posts.len();
            spawn(app, &ctx, "Rendering sheet", move |r| {
                for (i, p) in posts.iter().enumerate() {
                    if r.cancelled() {
                        break;
                    }
                    r.progress(format!("{}/{n}: images", i + 1));
                    let mut plan = photocraft_press::csv::to_plan(p, &b, fmt.key());
                    fill_images(&home, &env, &source, &mut plan, "");
                    let (k, sched) = (key.clone(), p.schedule.clone());
                    r.apply(move |app| {
                        let keep = app.press.batch_keep_open;
                        match render_post(app, &plan, &k, keep) {
                            Ok(e) => {
                                // A schedule column puts the post straight on the calendar.
                                if let (Some(when), Some(home)) = (schedule::parse_when(&sched), app.press.home.clone()) {
                                    let id = library::s(&e, "id").to_string();
                                    let _ = library::update_entry(&home, &id, |e| {
                                        e.insert("status".into(), json!("scheduled"));
                                        e.insert("scheduled_at".into(), json!(schedule::iso(when)));
                                        Ok(())
                                    });
                                }
                                app.press.status = format!("Rendered {}/{n}.", i + 1);
                            }
                            Err(e) => app.press.error = e,
                        }
                        app.press.refresh_queue();
                    });
                }
            });
        }
    }
}

fn saved_ui(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let Some(home) = app.press.home.clone() else { return };
    if ui.button("Refresh").clicked() || app.press.saved.is_empty() {
        app.press.saved = library::list_plans(&home, &app.press.brand);
    }
    if app.press.saved.is_empty() {
        ui.label("No saved plans for this brand yet (Post ▸ Save plan).");
    }
    let mut open = None;
    for (p, title) in &app.press.saved {
        if ui.link(title).clicked() {
            open = Some(p.clone());
        }
    }
    if let Some(p) = open {
        match library::load_plan(&p) {
            Ok(plan) => open_plan(app, plan),
            Err(e) => app.press.error = e.0,
        }
    }
}

// ── Post (plan editor) ───────────────────────────────────────────────────────

fn field(ui: &mut egui::Ui, label: &str, v: &mut String, multiline: bool) -> bool {
    ui.label(RichText::new(label).small().weak());
    let w = if multiline { egui::TextEdit::multiline(v).desired_rows(3) } else { egui::TextEdit::singleline(v) };
    ui.add(w.desired_width(f32::INFINITY)).changed()
}

fn image_row(app: &mut PhotocraftApp, ui: &mut egui::Ui, slot: usize, query: &mut String, image: &mut String) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        ui.label("Image");
        ui.add(egui::TextEdit::singleline(query).hint_text("image search").desired_width(150.0));
        if ui.add_enabled(!app.press.busy() && !query.trim().is_empty(), egui::Button::new("Find").small()).clicked() {
            let (Some(home), q, src) = (app.press.home.clone(), query.clone(), app.press.image_source.clone()) else { return };
            spawn(app, &ctx, "Fetching image", move |r| {
                let res = images::fetch_for_query(&home, &home.env(), &src, &q);
                r.apply(move |app| match res {
                    Ok(p) => set_image(app, slot, p.to_string_lossy().replace('\\', "/")),
                    Err(e) => app.press.error = e.0,
                });
            });
        }
        if ui.add_enabled(!app.press.busy(), egui::Button::new("File…").small()).clicked() {
            let r = app.ask_file(crate::file_dialog::FileDialogRequest::Open { multiple: false, initial_dir: None, extensions: Some(vec!["png".into(), "jpg".into(), "jpeg".into(), "webp".into()]) }, move |app, a| {
                if let crate::file_dialog::FileDialogAnswer::Paths(p) = a
                    && let Some(p) = p.first()
                {
                    set_image(app, slot, p.replace('\\', "/"));
                }
                Ok(Value::Null)
            });
            if let Err(e) = r
                && e != "cancelled"
            {
                app.press.error = e;
            }
        }
        if !image.is_empty() && ui.small_button("×").on_hover_text("No image").clicked() {
            image.clear();
        }
    });
    ui.horizontal(|ui| {
        let link = app.press.image_links.entry(slot).or_default();
        ui.add(egui::TextEdit::singleline(link).hint_text("or paste an image link").desired_width(200.0));
        let l = link.trim().to_string();
        if ui.add_enabled(!l.is_empty() && !app.press.busy(), egui::Button::new("Use").small()).clicked() {
            let Some(home) = app.press.home.clone() else { return };
            spawn(app, &ctx, "Downloading image", move |r| {
                let res = images::download(&home, &l, "url", &format!("slot-{slot}-{}", l.len()));
                r.apply(move |app| match res {
                    Ok(p) => {
                        set_image(app, slot, p.to_string_lossy().replace('\\', "/"));
                        app.press.image_links.remove(&slot);
                    }
                    Err(e) => app.press.error = e.0,
                });
            });
        }
    });
    if !image.is_empty() {
        let name = std::path::Path::new(image.as_str()).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        ui.label(RichText::new(format!("using {name}")).small().weak());
    }
}

/// Slot 0 = the post's main image (title / single card); n = content slide n-1.
fn set_image(app: &mut PhotocraftApp, slot: usize, path: String) {
    if let Some(p) = &mut app.press.plan {
        if slot == 0 {
            p.image = path;
        } else if let Some(s) = p.content_slides.get_mut(slot - 1) {
            s.image = path;
        }
    }
}

fn post_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let Some(mut plan) = app.press.plan.take() else {
        ui.label("No post open. Start one in Create, or open one from the Queue.");
        return;
    };
    ui.horizontal(|ui| {
        let mut f = plan.format();
        format_combo(ui, "press-post-fmt", &mut f);
        if f != plan.format() {
            plan.format = f.key().into();
            if f != Format::Carousel && plan.headline.is_empty() {
                plan.headline = plan.title_card.headline.clone();
                plan.body = plan.content_slides.first().map(|s| s.body.clone()).unwrap_or_default();
            }
        }
        let style_label = app.press.style.map_or("Brand look", |s| s.label());
        egui::ComboBox::from_id_salt("press-style").selected_text(style_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.press.style, None, "Brand look");
            ui.selectable_value(&mut app.press.style, Some(Style::Classic), "Classic");
            ui.selectable_value(&mut app.press.style, Some(Style::ImageForward), "Image-forward");
        });
    });
    let brand = app.press.brand.clone();
    let built_alive: Vec<(DocId, String)> = app.press.built.iter().filter(|(id, _)| doc_index(app, *id).is_some()).cloned().collect();
    app.press.built = built_alive;
    ui.horizontal_wrapped(|ui| {
        let label = if app.press.built.is_empty() { "Build in PhotoCraft" } else { "Rebuild" };
        if ui.add_enabled(!app.press.busy(), egui::Button::new(RichText::new(label).strong())).on_hover_text("Open every slide as an editable document").clicked() {
            let old = std::mem::take(&mut app.press.built);
            close_docs(app, &old);
            match build_docs(app, &plan, &brand, app.press.style) {
                Ok((docs, warnings)) => {
                    app.press.status = format!("Built {} slide(s). Edit them freely, then Export.", docs.len());
                    if !warnings.is_empty() {
                        app.press.error = format!("{} warning(s): {}", warnings.len(), warnings.join("; "));
                    }
                    app.press.built = docs;
                }
                Err(e) => app.press.error = e,
            }
        }
        let n = app.press.built.len();
        let export_label = if n == 0 { "Render & add to queue".to_string() } else { format!("Export {n} edited slide(s) to queue") };
        if ui.add_enabled(!app.press.busy(), egui::Button::new(export_label)).clicked() {
            let r = if app.press.built.is_empty() { render_post(app, &plan, &brand, false) } else { export_docs(app, &plan, &brand, &app.press.built.clone()) };
            match r {
                Ok(e) => {
                    app.press.status = format!("Added to the queue: {} ({}).", library::s(&e, "title"), library::s(&e, "rel"));
                    app.press.refresh_queue();
                }
                Err(e) => app.press.error = e,
            }
        }
        if ui.add_enabled(!app.press.built.is_empty(), egui::Button::new("Close slides")).clicked() {
            let old = std::mem::take(&mut app.press.built);
            close_docs(app, &old);
        }
    });
    ui.separator();
    let fmt = plan.format();
    let mut q0 = plan.image_query.clone();
    let mut i0 = plan.image.clone();
    match fmt {
        Format::Carousel => {
            section(ui, "Title slide");
            field(ui, "Headline", &mut plan.title_card.headline, false);
            field(ui, "Subhead", &mut plan.title_card.subhead, true);
            image_row(app, ui, 0, &mut q0, &mut i0);
            let mut remove = None;
            let mut up = None;
            let n = plan.content_slides.len();
            for i in 0..n {
                let Some(s) = plan.content_slides.get_mut(i) else { continue };
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Slide {}", i + 2)).strong());
                    if i > 0 && ui.small_button("↑").clicked() {
                        up = Some(i);
                    }
                    if n > 1 && ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
                field(ui, "Heading", &mut s.heading, false);
                field(ui, "Body (— one point per line)", &mut s.body, true);
                let (mut q, mut im) = (s.image_query.clone(), s.image.clone());
                image_row(app, ui, i + 1, &mut q, &mut im);
                if let Some(s) = plan.content_slides.get_mut(i) {
                    s.image_query = q;
                    // A Find / File… / link result lands in app.press.plan, which is taken out
                    // this frame: it is merged below.
                    if s.image != im && im.is_empty() {
                        s.image.clear();
                    }
                }
            }
            if let Some(i) = remove {
                plan.content_slides.remove(i);
            }
            if let Some(i) = up {
                plan.content_slides.swap(i, i - 1);
            }
            if plan.content_slides.len() < 8 && ui.button("+ Add slide").clicked() {
                plan.content_slides.push(ContentSlide { heading: "NEW POINT".into(), body: "— ".into(), ..Default::default() });
            }
            ui.separator();
            section(ui, "Last slide");
            field(ui, "Call to action", &mut plan.outro_card.cta, false);
            field(ui, "DM keyword (optional)", &mut plan.dm_keyword, false);
        }
        Format::Quote => {
            section(ui, "Quote card");
            field(ui, "Quote", &mut plan.quote, true);
            field(ui, "Attribution", &mut plan.subhead, false);
        }
        Format::Cover => {
            ui.label("The brand cover uses the brand's name, tagline, services and logo from config.yaml.");
        }
        _ => {
            section(ui, "Card");
            field(ui, "Headline", &mut plan.headline, false);
            field(ui, "Body", &mut plan.body, true);
            if fmt == Format::Story {
                field(ui, "Button text", &mut plan.cta, false);
            }
            if fmt == Format::X {
                field(ui, "Tweet text", &mut plan.tweet_text, true);
            }
            image_row(app, ui, 0, &mut q0, &mut i0);
        }
    }
    plan.image_query = q0;
    if i0.is_empty() {
        plan.image.clear();
    }
    ui.separator();
    section(ui, "Caption");
    ui.add(egui::TextEdit::multiline(&mut plan.caption).desired_rows(4).desired_width(f32::INFINITY));
    let mut tags = plan.hashtags.join(" ");
    ui.label(RichText::new("Hashtags").small().weak());
    if ui.add(egui::TextEdit::singleline(&mut tags).desired_width(f32::INFINITY)).changed() {
        plan.hashtags = photocraft_press::text::split_tags(&tags);
    }
    ui.label(RichText::new(format!("{} characters with hashtags", plan.full_caption().chars().count())).small().weak());
    ui.horizontal(|ui| {
        if ui.add_enabled(!app.press.busy(), egui::Button::new("Rewrite caption (AI)").small()).clicked()
            && let (Some(ai), Some(b)) = (app.press.backend(), app.press.brand())
        {
            let p = plan.clone();
            spawn(app, &ctx, "Writing caption", move |r| {
                let res = planner::regen_caption(&ai, &b, &p);
                r.apply(move |app| match (res, &mut app.press.plan) {
                    (Ok((c, t)), Some(pl)) => {
                        pl.caption = c;
                        pl.hashtags = t;
                    }
                    (Err(e), _) => app.press.error = e.0,
                    _ => {}
                });
            });
        }
        if ui.add_enabled(!app.press.busy(), egui::Button::new("Find all images").small()).clicked()
            && let Some(home) = app.press.home.clone()
        {
            let mut p = plan.clone();
            let src = app.press.image_source.clone();
            spawn(app, &ctx, "Fetching images", move |r| {
                let story_image = p.source_image.clone();
                fill_images(&home, &home.env(), &src, &mut p, &story_image);
                r.apply(move |app| {
                    if let Some(pl) = &mut app.press.plan {
                        if pl.image.is_empty() {
                            pl.image = p.image;
                        }
                        for (a, b) in pl.content_slides.iter_mut().zip(p.content_slides) {
                            if a.image.is_empty() {
                                a.image = b.image;
                            }
                        }
                    }
                    app.press.status = "Images fetched.".into();
                });
            });
        }
    });
    ui.separator();
    // Edits made this frame win; image results that arrived meanwhile are merged in.
    if let Some(arrived) = app.press.plan.take() {
        if !arrived.image.is_empty() && plan.image.is_empty() {
            plan.image = arrived.image;
        }
        for (a, b) in plan.content_slides.iter_mut().zip(arrived.content_slides) {
            if a.image.is_empty() && !b.image.is_empty() {
                a.image = b.image;
            }
        }
        if plan.caption.is_empty() {
            plan.caption = arrived.caption;
        }
    }
    let brand = app.press.brand.clone();
    ui.horizontal(|ui| {
        if ui.button("Save plan").clicked()
            && let Some(home) = app.press.home.clone()
        {
            match library::save_plan(&home, &brand, &plan) {
                Ok(p) => app.press.status = format!("Saved {}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()),
                Err(e) => app.press.error = e.0,
            }
            app.press.saved.clear();
        }
        if ui.button("Discard post").clicked() {
            app.press.built.clear();
            return;
        }
        app.press.plan = Some(plan.clone());
    });
    if app.press.plan.is_none() && !app.press.built.is_empty() {
        app.press.built.clear();
    }
    if !app.press.built.is_empty() {
        section(ui, "Slides");
        let mut go = None;
        for (id, name) in &app.press.built {
            if ui.link(name).clicked() {
                go = doc_index(app, *id);
            }
        }
        if let Some(i) = go {
            app.session.set_active(i);
        }
    }
}

// ── Queue ────────────────────────────────────────────────────────────────────

fn status_color(s: &str) -> Color32 {
    match s {
        "pending" => Color32::from_rgb(220, 180, 70),
        "approved" => Color32::from_rgb(100, 160, 230),
        "scheduled" => Color32::from_rgb(160, 120, 230),
        "published" => Color32::from_rgb(90, 190, 120),
        "failed" => Color32::from_rgb(235, 90, 80),
        _ => Color32::GRAY,
    }
}

fn thumb(app: &mut PhotocraftApp, ctx: &egui::Context, path: &std::path::Path) -> Option<egui::TextureHandle> {
    if let Some(t) = app.press.thumbs.get(path) {
        return t.clone();
    }
    let tex = (|| {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() > 40 * 1024 * 1024 {
            return None;
        }
        let img = photocraft_codecs::decode(&bytes).ok()?.convert(photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8);
        let (w, h) = (img.width() as usize, img.height() as usize);
        if img.data().len() != w * h * 4 || w == 0 || h == 0 {
            return None;
        }
        // Downsample to ~96 px tall (nearest), plenty for a list thumbnail.
        let step = (h / 96).max(1);
        let (tw, th) = (w / step, h / step);
        let mut px = Vec::with_capacity(tw * th * 4);
        for y in 0..th {
            for x in 0..tw {
                let i = ((y * step) * w + x * step) * 4;
                px.extend_from_slice(img.data().get(i..i + 4)?);
            }
        }
        Some(ctx.load_texture(path.to_string_lossy(), egui::ColorImage::from_rgba_unmultiplied([tw, th], &px), egui::TextureOptions::LINEAR))
    })();
    if app.press.thumbs.len() > 300 {
        app.press.thumbs.clear();
    }
    app.press.thumbs.insert(path.to_path_buf(), tex.clone());
    tex
}

fn set_status(app: &mut PhotocraftApp, id: &str, f: impl FnOnce(&mut Entry) -> Result<(), String>) {
    let Some(home) = app.press.home.clone() else { return };
    let r = library::update_entry(&home, id, |e| f(e).map_err(photocraft_studio::StudioError));
    if let Err(e) = r {
        app.press.error = e.0;
    }
    app.press.refresh_queue();
}

fn queue_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ui.horizontal_wrapped(|ui| {
        for (k, l) in [("", "All"), ("pending", "To review"), ("approved", "Approved"), ("scheduled", "Scheduled"), ("published", "Published"), ("failed", "Failed"), ("rejected", "Rejected")] {
            if ui.selectable_label(app.press.queue_filter == k, l).clicked() {
                app.press.queue_filter = k.into();
            }
        }
        if ui.small_button("Reload").on_hover_text("Reload the queue").clicked() {
            app.press.refresh_queue();
            app.press.thumbs.clear();
        }
    });
    let brand = app.press.brand.clone();
    let filter = app.press.queue_filter.clone();
    let items: Vec<Entry> = app.press.queue.iter().filter(|e| library::s(e, "brand") == brand && (filter.is_empty() || library::s(e, "status") == filter)).take(60).cloned().collect();
    if items.is_empty() {
        ui.label("Nothing here for this brand.");
    }
    let Some(home) = app.press.home.clone() else { return };
    for e in items {
        let id = library::s(&e, "id").to_string();
        let status = library::s(&e, "status").to_string();
        ui.separator();
        ui.horizontal(|ui| {
            let files = library::entry_files(&home, &e);
            if let Some(first) = files.first()
                && let Some(t) = thumb(app, &ctx, first)
            {
                let size = t.size_vec2();
                let k = 72.0 / size.y.max(1.0);
                if ui.add(egui::Image::new(&t).fit_to_exact_size(size * k).sense(egui::Sense::click())).on_hover_text("Open the slides in PhotoCraft").clicked() {
                    let paths: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
                    app.open_paths(&paths);
                }
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(library::s(&e, "title")).strong());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&status).color(status_color(&status)).small());
                    ui.label(RichText::new(format!("{} · {} slide(s)", library::s(&e, "format"), files.len())).small().weak());
                });
                let when = library::s(&e, "scheduled_at");
                if !when.is_empty() {
                    ui.label(RichText::new(format!("When: {}", when.replace('T', " "))).small());
                }
                if let Some(r) = e.get("blocked_reason").and_then(Value::as_str) {
                    ui.label(RichText::new(format!("⚠ {r}")).small().color(Color32::from_rgb(220, 180, 70)));
                }
                if let Some(p) = e.get("publish") {
                    for err in p.get("errors").and_then(Value::as_array).into_iter().flatten() {
                        ui.label(RichText::new(format!("× {}: {}", err.get("platform").and_then(Value::as_str).unwrap_or(""), err.get("error").and_then(Value::as_str).unwrap_or(""))).small().color(Color32::from_rgb(235, 90, 80)));
                    }
                    for r in p.get("results").and_then(Value::as_array).into_iter().flatten() {
                        if let Some(link) = r.get("permalink").and_then(Value::as_str).filter(|l| !l.is_empty()) {
                            ui.hyperlink_to(RichText::new(format!("{} →", r.get("platform").and_then(Value::as_str).unwrap_or("post"))).small(), link);
                        }
                    }
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            if status == "pending" {
                if ui.small_button("✔ Approve").clicked() {
                    set_status(app, &id, |e| {
                        e.insert("status".into(), json!("approved"));
                        e.insert("approved_at".into(), json!(library::now_iso()));
                        Ok(())
                    });
                }
                if ui.small_button("× Reject").clicked() {
                    set_status(app, &id, |e| {
                        e.insert("status".into(), json!("rejected"));
                        Ok(())
                    });
                }
            }
            if let Some(plan) = library::entry_plan(&home, &e)
                && ui.small_button("Edit").on_hover_text("Open the post's plan to change and re-render it").clicked()
            {
                open_plan(app, plan);
            }
            if matches!(status.as_str(), "approved" | "scheduled" | "failed" | "pending") {
                let slot = app.press.when.entry(id.clone()).or_insert_with(|| library::s(&e, "scheduled_at").chars().take(16).collect::<String>().replace('T', " "));
                ui.add(egui::TextEdit::singleline(slot).hint_text("YYYY-MM-DD HH:MM").desired_width(120.0));
                if ui.small_button("Next slot").clicked()
                    && let Some(cfg) = &app.press.cfg
                    && let Some(n) = schedule::next_slots(cfg, &app.press.queue, 1, schedule::now()).first()
                {
                    *app.press.when.entry(id.clone()).or_default() = n.format("%Y-%m-%d %H:%M").to_string();
                }
                if ui.small_button("Schedule").clicked() {
                    let v = app.press.when.get(&id).cloned().unwrap_or_default();
                    match schedule::parse_when(&v) {
                        Some(w) => set_status(app, &id, |e| {
                            e.insert("status".into(), json!("scheduled"));
                            e.insert("scheduled_at".into(), json!(schedule::iso(w)));
                            if !e.contains_key("approved_at") {
                                e.insert("approved_at".into(), json!(library::now_iso()));
                            }
                            Ok(())
                        }),
                        None => app.press.error = "Type the time as YYYY-MM-DD HH:MM (or press Next slot).".into(),
                    }
                }
                if status == "scheduled" && ui.small_button("Unschedule").clicked() {
                    set_status(app, &id, |e| {
                        e.insert("status".into(), json!("approved"));
                        e.remove("scheduled_at");
                        Ok(())
                    });
                }
            }
            if !matches!(status.as_str(), "published" | "rejected") {
                if app.press.confirm_publish.as_deref() == Some(id.as_str()) {
                    ui.label(RichText::new("Publish now?").strong());
                    if ui.small_button("Yes, publish").clicked() {
                        app.press.confirm_publish = None;
                        publish(app, &ctx, vec![id.clone()]);
                    }
                    if ui.small_button("Cancel").clicked() {
                        app.press.confirm_publish = None;
                    }
                } else if ui.small_button("Publish now").clicked() {
                    app.press.confirm_publish = Some(id.clone());
                }
            }
            if ui.small_button("Files").on_hover_text("Show the files").clicked() {
                let dir = home.outputs().join(library::s(&e, "rel"));
                reveal(app, &ctx, &dir);
            }
        });
    }
}

/// Show a folder in the system file manager.
fn reveal(app: &mut PhotocraftApp, ctx: &egui::Context, dir: &std::path::Path) {
    let url = format!("file:///{}", dir.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/").trim_start_matches('/'));
    match &app.services.open_url {
        Some(open) => {
            let _ = open(&url);
        }
        None => ctx.open_url(egui::OpenUrl::new_tab(url)),
    }
}

/// Publish entries on a worker and stamp the results on the queue.
fn publish(app: &mut PhotocraftApp, ctx: &egui::Context, ids: Vec<String>) {
    let (Some(home), Some(cfg)) = (app.press.home.clone(), app.press.cfg.clone()) else { return };
    spawn(app, ctx, "Publishing", move |r| {
        let env = home.env();
        let mut summary = Vec::new();
        for id in ids {
            if r.cancelled() {
                break;
            }
            // Re-read right before sending: the web app may have published it meanwhile.
            let fresh = library::load_queue(&home).ok().and_then(|q| q.into_iter().find(|e| library::s(e, "id") == id));
            let Some(mut e) = fresh else { continue };
            if library::s(&e, "status") == "published" {
                continue;
            }
            r.progress(library::s(&e, "title").to_string());
            let res = meta::publish_entry(&home, &cfg, &env, &mut e);
            let title = library::s(&e, "title").to_string();
            let _ = library::update_entry(&home, &id, |x| {
                *x = e.clone();
                Ok(())
            });
            summary.push(if res.get("sent").and_then(Value::as_bool) == Some(true) {
                format!("Published “{title}”.")
            } else {
                format!("“{title}” not published: {}", res.get("reason").and_then(Value::as_str).map(str::to_string).or_else(|| res.get("errors").map(Value::to_string)).unwrap_or_default())
            });
        }
        r.apply(move |app| {
            app.press.status = summary.join(" ");
            app.press.refresh_queue();
        });
    });
}

/// With Settings ▸ Run the scheduler here on: publish due calendar posts once a minute.
fn scheduler_tick(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.press.settings.run_scheduler || app.press.busy() {
        return;
    }
    ctx.request_repaint_after(Duration::from_secs(30));
    if app.press.last_tick.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
        return;
    }
    app.press.last_tick = Some(Instant::now());
    let (Some(home), Some(cfg)) = (&app.press.home, &app.press.cfg) else { return };
    if !schedule::slots(cfg).auto_publish {
        return;
    }
    let now = schedule::now();
    let due: Vec<String> = library::load_queue(home).unwrap_or_default().iter().filter(|e| schedule::is_due(e, now)).map(|e| library::s(e, "id").to_string()).collect();
    if !due.is_empty() {
        publish(app, ctx, due);
    }
}

// ── Calendar ─────────────────────────────────────────────────────────────────

fn calendar_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    use photocraft_studio::chrono::Datelike;
    let (y, m) = app.press.cal;
    ui.horizontal(|ui| {
        if ui.button("◀").clicked() {
            app.press.cal = schedule::shift_month(y, m, -1);
        }
        let name = photocraft_studio::chrono::NaiveDate::from_ymd_opt(y, m, 1).map(|d| d.format("%B %Y").to_string()).unwrap_or_default();
        ui.label(RichText::new(name).strong());
        if ui.button("▶").clicked() {
            app.press.cal = schedule::shift_month(y, m, 1);
        }
        if ui.button("Today").clicked() {
            let n = schedule::now();
            app.press.cal = (n.year(), n.month());
        }
    });
    let brand = app.press.brand.clone();
    let mut by_day: BTreeMap<photocraft_studio::chrono::NaiveDate, Vec<(String, String, String)>> = BTreeMap::new();
    for e in &app.press.queue {
        if library::s(e, "brand") != brand {
            continue;
        }
        let when = schedule::parse_when(library::s(e, "scheduled_at")).or_else(|| if library::s(e, "status") == "published" { schedule::parse_when(library::s(e, "published_at")) } else { None });
        if let Some(w) = when {
            by_day.entry(w.date()).or_default().push((w.format("%H:%M").to_string(), library::s(e, "title").to_string(), library::s(e, "status").to_string()));
        }
    }
    let today = schedule::now().date();
    let cell_w = ((ui.available_width() - 12.0) / 7.0).max(40.0);
    egui::Grid::new("press-cal").num_columns(7).spacing([2.0, 2.0]).min_col_width(cell_w).max_col_width(cell_w).show(ui, |ui| {
        for d in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
            ui.label(RichText::new(d).small().weak());
        }
        ui.end_row();
        for (i, day) in schedule::month_grid(y, m).into_iter().enumerate() {
            ui.vertical(|ui| {
                ui.set_min_height(54.0);
                let txt = RichText::new(day.day().to_string()).small();
                let txt = if day == today { txt.strong().color(Color32::from_rgb(100, 160, 230)) } else if day.month() != m { txt.weak() } else { txt };
                ui.label(txt);
                for (t, title, st) in by_day.get(&day).into_iter().flatten().take(3) {
                    ui.label(RichText::new(format!("{t} {}", photocraft_press::text::clip(title, 14))).small().color(status_color(st))).on_hover_text(format!("{t} · {st}\n{title}"));
                }
            });
            if i % 7 == 6 {
                ui.end_row();
            }
        }
    });
    ui.separator();
    let approved: Vec<String> = app.press.queue.iter().filter(|e| library::s(e, "brand") == brand && library::s(e, "status") == "approved").map(|e| library::s(e, "id").to_string()).collect();
    if ui.add_enabled(!approved.is_empty(), egui::Button::new(format!("Fill free slots with {} approved post(s)", approved.len()))).clicked()
        && let (Some(home), Some(cfg)) = (app.press.home.clone(), app.press.cfg.clone())
    {
        let slots = schedule::next_slots(&cfg, &app.press.queue, approved.len(), schedule::now());
        for (id, at) in approved.iter().zip(slots) {
            let _ = library::update_entry(&home, id, |e| {
                e.insert("status".into(), json!("scheduled"));
                e.insert("scheduled_at".into(), json!(schedule::iso(at)));
                Ok(())
            });
        }
        app.press.refresh_queue();
        app.press.status = "Approved posts placed on the calendar.".into();
    }
    if let Some(cfg) = &app.press.cfg {
        let sl = schedule::slots(cfg);
        let times: Vec<String> = sl.times.iter().map(|t| t.format("%H:%M").to_string()).collect();
        let days: Vec<String> = sl.days.iter().map(|d| d.to_string()).collect();
        ui.label(RichText::new(format!("Posting slots: {} on {} (config.yaml ▸ schedule).", times.join(", "), days.join(" "))).small().weak());
        if !app.press.settings.run_scheduler {
            ui.label(RichText::new("Scheduled posts are sent by the web app's scheduler, or turn on Settings ▸ Run the scheduler here.").small().weak());
        }
    }
}

// ── Settings ─────────────────────────────────────────────────────────────────

fn settings_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !app.press.load_error.is_empty() {
        ui.colored_label(Color32::from_rgb(235, 90, 80), &app.press.load_error);
    }
    section(ui, "K2 Press folder");
    ui.label(RichText::new("The folder with config.yaml, .env, library and outputs. Shared with the K2 Press web app.").small().weak());
    ui.add(egui::TextEdit::singleline(&mut app.press.home_input).desired_width(f32::INFINITY));
    ui.horizontal(|ui| {
        if ui.button("Browse…").clicked() {
            let r = app.ask_folder(|app, path| {
                app.press.home_input = path;
                apply_home(app);
                Ok(Value::Null)
            });
            if let Err(e) = r
                && e != "cancelled"
            {
                app.press.error = e;
            }
        }
        if ui.button("Use this folder").clicked() {
            apply_home(app);
        }
        if ui.button("Reload config").clicked() {
            app.press.reload();
            app.press.status = "Reloaded.".into();
        }
    });
    let Some(cfg) = app.press.cfg.clone() else { return };
    section(ui, "Local AI");
    let names = Backend::names(&cfg);
    ui.horizontal(|ui| {
        let cur = if app.press.settings.backend.is_empty() { "config.yaml default".to_string() } else { app.press.settings.backend.clone() };
        let mut changed = false;
        egui::ComboBox::from_id_salt("press-ai").selected_text(cur).show_ui(ui, |ui| {
            changed |= ui.selectable_value(&mut app.press.settings.backend, String::new(), "config.yaml default").changed();
            for n in &names {
                changed |= ui.selectable_value(&mut app.press.settings.backend, n.clone(), n).changed();
            }
        });
        ui.label("Model");
        changed |= ui.add(egui::TextEdit::singleline(&mut app.press.settings.model).hint_text("backend default").desired_width(120.0)).lost_focus();
        if changed {
            let _ = app.press.settings.save();
        }
    });
    if let Some(b) = app.press.backend() {
        ui.label(RichText::new(format!("Using {}", b.name())).small().weak());
    }
    section(ui, "Images");
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.press.image_source, "pexels".to_string(), "Pexels");
        ui.radio_value(&mut app.press.image_source, "unsplash".to_string(), "Unsplash");
    });
    if let Some(home) = &app.press.home {
        let env = home.env();
        let key = if app.press.image_source == "unsplash" { "UNSPLASH_API_KEY" } else { "PEXELS_API_KEY" };
        if env.get(key).is_none() {
            ui.label(RichText::new(format!("{key} is not set in .env")).small().color(Color32::from_rgb(220, 180, 70)));
        }
    }
    section(ui, "Publishing");
    let mut sched = app.press.settings.run_scheduler;
    if ui.checkbox(&mut sched, "Run the scheduler here (publish due calendar posts)").on_hover_text("Leave off while the K2 Press web app's scheduler is running, so a post is never sent by both.").changed() {
        app.press.settings.run_scheduler = sched;
        let _ = app.press.settings.save();
    }
    if let Some(home) = &app.press.home {
        let env = home.env();
        for (k, what) in [("META_ACCESS_TOKEN", "Meta token"), ("PUBLIC_BASE_URL", "Public URL for Instagram images")] {
            let ok = env.get(k).is_some();
            ui.label(RichText::new(format!("{} {what} ({k})", if ok { "✔" } else { "×" })).small().color(if ok { Color32::from_rgb(90, 190, 120) } else { Color32::from_rgb(220, 180, 70) }));
        }
    }
    if ui.add_enabled(!app.press.busy(), egui::Button::new("Test the connection for this brand")).clicked()
        && let Some(home) = app.press.home.clone()
    {
        let brand = app.press.brand.clone();
        spawn(app, &ctx, "Checking Meta", move |r| {
            let res = meta::verify(&cfg, &home.env(), &brand);
            r.apply(move |app| {
                app.press.verify_text = match res {
                    Ok(t) => t,
                    Err(e) => format!("× {}", e.0),
                }
            });
        });
    }
    if !app.press.verify_text.is_empty() {
        ui.label(RichText::new(&app.press.verify_text).small());
    }
}

fn apply_home(app: &mut PhotocraftApp) {
    let path = app.press.home_input.trim().to_string();
    match Home::new(&path) {
        Ok(_) => {
            app.press.settings.home = path;
            if let Err(e) = app.press.settings.save() {
                app.press.error = e.0;
            }
            app.press.reload();
            app.press.status = "K2 Press folder set.".into();
        }
        Err(e) => app.press.error = e.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> PhotocraftApp {
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
    }

    #[test]
    fn built_and_closed_slides_keep_one_view_per_document() {
        let mut app = app();
        let plan = Plan::from_json(&json!({"title_card": {"headline": "Hello"}, "content_slides": [{"heading": "A", "body": "— b"}], "outro_card": {"cta": "Follow"}})).unwrap();
        let (docs, warnings) = build_docs(&mut app, &plan, "nobody", None).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(docs.len(), 3);
        assert_eq!(app.ui.views.len(), app.session.documents().len());
        close_docs(&mut app, &docs[..2]);
        assert_eq!(app.session.documents().len(), 1);
        assert_eq!(app.ui.views.len(), 1);
    }

    #[test]
    fn export_without_a_library_is_an_error() {
        let mut app = app();
        let plan = Plan::default();
        assert!(export_docs(&mut app, &plan, "k2", &[]).is_err());
    }
}
