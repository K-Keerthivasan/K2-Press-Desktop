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

## Cloud scheduling and posting (Supabase)

Post Studio uses the same Supabase backend as Shorts Factory Desktop (`publishing-api` edge
function, Backblaze B2 or Supabase Storage). In the Queue:

- **Schedule in cloud**: uploads the slides and schedules the post at the typed time. It
  publishes even while this PC is off.
- **Post now**: uploads and asks the cloud to publish right away. Instagram fetches the images
  from short-lived signed storage URLs, so no `PUBLIC_BASE_URL` or running web app is needed.
- **Sync cloud** pulls results back (published + permalink, failed + reason, cancelled).
- Cloud posts can be cancelled or moved to now until publishing starts.

Slides are converted to JPEG (Instagram's content publishing takes JPEG only, at most 8 MB) into
`outputs/<rel>/cloud/`. Single images, stories and carousels (2–10) are supported. A cloud
post's queue status is `cloud_scheduled`, which the web app's own scheduler never sends, so a post
is never published twice.

Credentials: `CLOUD_SUPABASE_URL`, `CLOUD_SUPABASE_ANON_KEY`, `CLOUD_AUTH_EMAIL`,
`CLOUD_AUTH_PASSWORD` from the K2 Press `.env`, or else from the Shorts Factory Desktop `.env`
(`%APPDATA%\Shorts Factory Desktop\.env`). Pick each brand's cloud Instagram account in
**Settings › Cloud** (matched automatically by `meta.accounts.<brand>.ig_user_id`).

**Backend update required.** Image posts need the backend changes in
`E:\AI\IG_ReelAutomation\cloud` (migration `202610150001_image_posts.sql` and the updated
`publishing-api`, `publish-due-posts` and `cleanup-orphans` functions; Reels are unchanged).
Run `deno test` there, then apply the migration and deploy the three functions. Until then the
cloud refuses image posts with a clear error.

## Clients, looks and platforms

Brands in `config.yaml` are **personal** (default: K2 Digital Media, JKR) or **clients**
(`kind: client`, `client_of: K2 Digital Media`, e.g. Phoenix Fine Touch). Clients show as
"· client" in the brand picker. Each brand picks its look with `style`:

| Look | For | What it draws |
|---|---|---|
| `classic` | K2 Digital Media | dark panel, accent rule, logo lockup, faint photo |
| `imageForward` | JKR | full-bleed photo, heavy uppercase type, accent bar |
| `service` | local-business clients (Phoenix) | light room photo, wide logo, script tagline, kicker band, teal headline with gold underline, checklist card, services row, contact footer bar |

Client fields: `phone`, `address`, `location`, `website`, `script_tagline`, `script_font`,
`service_items` (`{label, icon}`; icons `house leaf sparkle shield star check broom spray calendar`,
or your own `static/icons/<name>.png|svg`) and `platforms`.

Every look renders every platform format: Instagram carousel, post (4:5), square, story / Reel
cover, Facebook, LinkedIn, X, YouTube thumbnail, YouTube channel banner (text in the safe area),
email header, Pinterest, quote card and brand cover. **Post ▸ Make all versions** renders the
brand's `platforms` into `outputs/<brand>/<date>_<slug>_versions/<format>/`, plus `email.html`
(a 600 px table newsletter around the email header, ready for Mailchimp or Gmail).

## MCP (agents drive the app)

The app opens its JSON control channel on `127.0.0.1:9877` with the token in
`%APPDATA%\K2 Press Desktop\control.token` (skipped if the port is busy). The `k2-press` MCP
server bridges to it:

```json
"k2-press": { "type": "stdio", "command": "%APPDATA%\K2 Press Desktop\tools\photocraft-cli.exe",
  "args": ["mcp", "--bridge", "127.0.0.1:9877", "--control-token-file", "%APPDATA%\K2 Press Desktop\control.token"] }
```

It exposes every PhotoCraft command plus `press.build` (plan + brand → editable slide documents),
`press.slides`, `press.formats`, `press.buildSpec` and `press.formatGallery`, and the UI tools
(screenshot, click, inspect). Without the app running, `photocraft-cli mcp` runs the same
commands headless.

## Publishing (direct, from this PC)

Same rules as K2 Press's `meta.py`: Instagram needs `META_ACCESS_TOKEN` and `PUBLIC_BASE_URL` (an
https address serving the K2 Press `outputs/` folder, since Meta fetches the images itself);
Facebook Pages get multipart uploads. The 24-hour rate guard file (`library/meta_rate.json`) is
shared with the web app. A post whose setup is incomplete keeps its calendar slot and says why.

The scheduler (publishing due calendar posts) runs here only when **Settings › Run the scheduler
here** is on. Leave it off while the web app's scheduler runs so a post is never sent twice.

## Build

```powershell
cargo run --release -p photocraft --bin k2-press-desktop
```

The executable is `target\release\k2-press-desktop.exe`.

Tests: `cargo test -p photocraft-press -p photocraft-studio` and
`cargo test -p photocraft-engine --test press`.

## Branding

The executable is `k2-press-desktop.exe` (window title "K2 Press", its own taskbar identity
`ca.k2digitalmedia.k2press`, settings in `%APPDATA%\K2 Press Desktop`). The app icon is original
(`packaging/k2press_icon.py`, see `assets/app-icon/README.md`). Crate names stay PhotoCraft's so
upstream merges stay clean.

## Next

- More layout recipes per brand (the web app's listicle, comparison, breaking, LinkedIn).
- Video / Reels (the brand `video:` blocks).
- Serve `outputs/` from the app so Instagram works without the web app.
