# Post Studio: K2 Press on PhotoCraft

K2 Press Desktop is a fork of [PhotoCraft](https://github.com/storytold/photocraft) (pure-Rust,
Photoshop-class image editor) with the K2 Press content studio built in. Every PhotoCraft feature is
still here (layers, masks, type, shapes, adjustment layers, layer styles, PSD, filters). Post Studio
adds social post making on top, as a panel to the left of the canvas (**Window › Post Studio**).

It reads the K2 Press folder (`E:\AI\K2_PostTool`) **in place**: `config.yaml`, `.env`,
`library/review_queue.json`, `library/plans/`, `library/used_urls.json`, `image_cache/` and
`outputs/`. Nothing is copied, so the web app and this one can run side by side.

## Architecture

| Layer | Crate / file | What it holds |
|---|---|---|
| L0 (standalone) | `crates/press` (`photocraft-press`) | Brand, post plan (same JSON as K2 Press), CSV import, the post layouts as engine command steps |
| L4 | `crates/studio` (`photocraft-studio`) | K2 Press folder + settings, config.yaml, Hermes / Ollama AI, RSS feeds, Pexels / Unsplash, review queue, schedule, Meta Graph publishing |
| L5 | `crates/engine/src/press_cmds.rs` | `press.build`, `press.buildSpec`, `press.slides`, `press.formats` engine commands |
| L6 | `crates/ui-egui/src/press_ui.rs` | The Post Studio panel |

A post plan compiles to one **slide spec** per slide: a canvas size and a list of ordinary engine
commands (`shape.create`, `type.create`, `file.placeEmbedded` + `edit.transform` for cover-fitted
photos clipped to a frame). `press.build` opens a new document per slide and runs them, so every
headline, panel, logo and photo is a normal editable layer. Because they are commands, the CLI,
the control channel and MCP can build posts too:

```powershell
photocraft-cli run --new '{"width":8,"height":8}' --cmd press.build --params '{"plan":{…},"brand":{…}}'
```

## Formats and looks

Carousel (1080×1350: title, 1–8 content slides, outro), Square (1080×1080), Story (1080×1920),
X (1600×900), Quote card and Brand cover. Two looks: **Classic** (K2 Digital Media: dark panel,
accent rule, logo lockup, faint photo) and **Image-forward** (JKR, `image_forward: true`:
full-bleed photo, heavy uppercase type, accent bar). Colours, font, handle, logo and tagline come
from the brand block in `config.yaml`.

## The panel

| Tab | What it does |
|---|---|
| **Create ▸ Idea** | Idea + notes → AI writes the post (manual-source isolation, as in K2 Press); Suggest angles; or start blank |
| **Create ▸ Stories** | Fetch a brand's RSS / YouTube feeds, AI-score 0–100, drop near-duplicates and already-used stories; make one post or generate a batch straight into the queue |
| **Create ▸ Sheet** | CSV import (wide or long sheets, loose column names): copy used verbatim, or AI writes from each row; a `schedule` column puts posts on the calendar |
| **Create ▸ Saved** | Saved plans (`library/plans/<brand>/`) |
| **Post** | Edit every line, slide order, images (search, file or link), caption and hashtags (AI rewrite); **Build in PhotoCraft** opens each slide as a document; edit them with any tool; **Export** writes the edited slides to `outputs/<brand>/<date>_<slug>_<format>/` and adds the post to the review queue (with `plan.json` beside it, so it can be reopened) |
| **Queue** | Review queue: approve, reject, edit, schedule (or next free slot), publish now (with confirmation), open slides in PhotoCraft, show files |
| **Calendar** | Month grid of scheduled and published posts; fill free slots with approved posts |
| **Settings** | K2 Press folder, AI backend and model, image source, scheduler, Meta connection test |

## Publishing

Same rules as K2 Press's `meta.py`: Instagram needs `META_ACCESS_TOKEN` and `PUBLIC_BASE_URL` (an
https address serving the K2 Press `outputs/` folder, since Meta fetches the images itself);
Facebook Pages get multipart uploads. The 24-hour rate guard file (`library/meta_rate.json`) is
shared with the web app. A post whose setup is incomplete keeps its calendar slot and says why.

The scheduler (publishing due calendar posts) runs here only when **Settings › Run the scheduler
here** is on. Leave it off while the web app's scheduler runs so a post is never sent twice.

## Build

```powershell
cargo run --release -p photocraft
```

Tests: `cargo test -p photocraft-press -p photocraft-studio` and
`cargo test -p photocraft-engine --test press`.

## Next

- Rename the app's branding (window title, icon, settings folder) to K2 Press Desktop.
- More layout recipes per brand (the web app's listicle, comparison, breaking, LinkedIn).
- Video / Reels (the brand `video:` blocks).
- Serve `outputs/` from the app so Instagram works without the web app.
