//! AI post planning and story scoring, with the same prompts as K2 Press's `plan.py` and
//! `filter.py`, so plans written here and there read the same.

use photocraft_press::{Brand, Plan};
use serde_json::{Value, json};

use crate::Story;
use crate::llm::Backend;
use crate::{Result, StudioError};

pub const PLATFORMS: &[(&str, &str, &str, usize, &str)] = &[
    (
        "instagram",
        "Instagram",
        "3-4 concise sentences with a strong opening line",
        10,
        "Make every slide highly visual and easy to scan on a phone. Use short, punchy copy and a save, share, follow, or DM call to action.",
    ),
    (
        "linkedin",
        "LinkedIn",
        "4-7 short, professional paragraphs with generous line breaks",
        3,
        "Write for a professional audience. Prioritise credible insight, business relevance, and a useful takeaway; avoid Instagram-style hype and engagement bait.",
    ),
    (
        "facebook",
        "Facebook",
        "2-4 conversational paragraphs that provide enough context on their own",
        3,
        "Use approachable, community-friendly language. Make the post understandable without swiping first and favour a natural question or discussion prompt when appropriate.",
    ),
    (
        "x",
        "X",
        "one sharp post under 240 characters",
        2,
        "Be exceptionally concise and direct. Keep graphic copy sparse, lead with the strongest claim, and avoid filler, long setup, and more than one call to action.",
    ),
];

pub const CONTENT_TYPES: &[(&str, &str, &str)] = &[
    (
        "general",
        "General",
        "Choose the clearest, most useful framing for the supplied material. Preserve its core meaning and do not force it into a promotional, opinion, or tutorial structure.",
    ),
    (
        "educational",
        "Educational / how-to",
        "Teach something practical. Organise the post into clear steps, principles, or mistakes, and make every slide give the reader an actionable takeaway.",
    ),
    (
        "thought_leadership",
        "Thought leadership / opinion",
        "Express a clear, defensible point of view. Explain why it matters, support it with the provided facts, and end with a useful implication for the audience.",
    ),
    (
        "promotional",
        "Promotional / offer",
        "Present the offer through audience benefits and a concrete problem it solves. Do not invent features, proof, pricing, urgency, or results; use one direct conversion-focused CTA.",
    ),
    (
        "announcement",
        "Announcement / update",
        "Lead with what is new, then explain who it affects, why it matters, and what the audience should do next. Keep the news clear instead of teasing it.",
    ),
    (
        "case_study",
        "Case study / results",
        "Structure the story as problem, approach, and result or lesson. Use only outcomes present in the supplied notes and never fabricate client names, metrics, quotes, or evidence.",
    ),
    (
        "storytelling",
        "Storytelling / lesson",
        "Build a concise narrative with a hook, tension or turning point, and a useful lesson. Keep the story grounded in the supplied information rather than inventing anecdotes.",
    ),
    (
        "business_tip",
        "Business tip / how-to",
        "Teach a practical lesson for a busy local business owner. Connect every recommendation to a concrete outcome such as leads, trust, speed, rankings, bookings, retention, or saved spend.",
    ),
    (
        "service_spotlight",
        "Service spotlight / offer",
        "Explain the audience problem first, then show how the relevant service solves it and what business benefit it creates. Do not invent features, pricing, proof, urgency, or results.",
    ),
    (
        "industry_insight",
        "Industry insight / trend",
        "Interpret the trend instead of merely reporting it. Explain what it changes for small and mid-sized businesses and give a practical next step.",
    ),
    (
        "project_showcase",
        "Project showcase / portfolio",
        "Show the work through its goal, the reasoning or craft behind it, and the value delivered. Use only project details and outcomes supplied by the user.",
    ),
    (
        "company_announcement",
        "Company announcement / update",
        "Lead clearly with the update, then explain who it affects, why it matters, and the one next action. Avoid inflated milestone language.",
    ),
    (
        "breaking_news",
        "Breaking news / update",
        "Lead with what happened, then quickly explain why it matters to players or viewers. Separate confirmed facts from rumours and do not bury the news behind a tease.",
    ),
    (
        "review",
        "Review / verdict",
        "Give a clear verdict supported by specific strengths, weaknesses, and audience fit. Do not invent hands-on experience, scores, quotes, or details not supplied by the user.",
    ),
    (
        "reaction",
        "Reaction / hot take",
        "Take a confident fan-first position, identify the stakes, and support the reaction with concrete reasoning. Be punchy without manufacturing outrage or hype.",
    ),
    (
        "trailer_breakdown",
        "Trailer / reveal breakdown",
        "Pull out the most important reveals, details, and implications in a logical order. Clearly mark interpretation or speculation and avoid presenting theories as confirmed facts.",
    ),
    (
        "ranking",
        "Ranking / list",
        "Create a punchy ranked or curated list. Give a distinct, specific reason for every choice and make the ranking criteria clear.",
    ),
    (
        "explainer",
        "Explainer / lore",
        "Make the subject easy for fans to follow by clarifying the essential context, connections, and takeaway without turning the post into a dry encyclopedia summary.",
    ),
    (
        "recommendation",
        "Recommendation / what to play or watch",
        "Recommend what to play or watch, explain exactly why, identify who it best suits, and mention any relevant caveat supported by the supplied information.",
    ),
];

/// What to plan from.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub story: Story,
    /// A manual brief: only the idea and notes are allowed as source.
    pub manual: bool,
    /// Total slides (3–10); `None` = from the story length.
    pub total_slides: Option<usize>,
    pub platform: String,
    pub content_type: String,
    pub tone: String,
    /// Run the second "brand personality" copy pass.
    pub polish: bool,
}

fn platform(key: &str) -> (&'static str, &'static str, &'static str, usize, &'static str) {
    PLATFORMS.iter().copied().find(|p| p.0 == key.trim().to_lowercase()).unwrap_or(PLATFORMS[0])
}

fn content(key: &str) -> (&'static str, &'static str, &'static str) {
    CONTENT_TYPES.iter().copied().find(|c| c.0 == key.trim().to_lowercase()).unwrap_or(CONTENT_TYPES[0])
}

fn voice(brand: &Brand) -> String {
    let p = brand.personality.trim();
    if p.is_empty() {
        String::new()
    } else {
        format!("\nBRAND PERSONALITY:\n{p}\nApply this voice to the actual post copy. Keep facts, schema, and source meaning intact.\n")
    }
}

fn tone_block(tone: &str) -> String {
    let t = tone.trim();
    if t.is_empty() {
        String::new()
    } else {
        format!(
            "\nTONE & PERSPECTIVE:\n- Take a {t} stance toward this topic.\n- Let that perspective shape the headline, the framing, and which points\n  you emphasise — while staying factual and true to the source story.\n"
        )
    }
}

fn profile(brand: &Brand) -> String {
    let p = brand.profile.trim();
    if p.is_empty() { "A social media brand.".into() } else { p.into() }
}

fn source(req: &Request) -> String {
    let s = &req.story;
    let summary = photocraft_press::text::clip(&s.summary, 900);
    if req.manual {
        format!("MANUAL BRIEF — the only allowed source:\nIdea / topic: {}\nUser notes:   {summary}", s.title)
    } else {
        format!("Story title:   {}\nStory summary: {summary}\nSource URL:    {}", s.title, s.url)
    }
}

const MANUAL_RULES: &str = "\nMANUAL SOURCE ISOLATION:\n- This is a manual brief, not an RSS or news-feed story.\n- Use ONLY the idea and notes in the MANUAL BRIEF below as factual source material.\n- Do not use stored stories, previous posts, RSS content, remembered current events, or outside facts.\n- Do not invent release dates, announcements, quotes, statistics, features, results, or plot details.\n- If the notes are sparse, stay general and useful instead of filling gaps with unrelated news.\n- Never replace the user's topic with a different or more newsworthy topic.\n";

/// A carousel plan (title + content slides + outro).
pub fn plan_carousel(ai: &Backend, brand: &Brand, req: &Request) -> Result<Plan> {
    let words = req.story.summary.split_whitespace().count();
    let n_content = match req.total_slides {
        Some(t) => t.saturating_sub(2).clamp(1, 8),
        None if words <= 220 => 2,
        None => 4,
    };
    let total = n_content + 2;
    let (pkey, pname, prule, ntags, prules) = platform(&req.platform);
    let (ckey, clabel, crules) = content(&req.content_type);
    let tags = json!(brand.hashtags.iter().take(ntags).collect::<Vec<_>>());
    let tone = if req.tone.trim().is_empty() { brand.tone.trim() } else { req.tone.trim() };
    let loc = if brand.location.is_empty() { String::new() } else { format!("- Make {} relevance explicit when it is not obvious.\n", brand.location) };
    let handle = brand.at_handle();
    let system = format!(
        r#"You are a {pname} carousel content planner for {name}.
You write clear, value-first posts — no fluff, no hype.

CREATOR PROFILE:
{profile}
{voice}
{tone}
TARGET PLATFORM: {pname}
PLATFORM DIRECTION:
{prules}

CONTENT TYPE: {clabel}
CONTENT DIRECTION:
{crules}
{manual}

Return ONE valid JSON object (no markdown, no code fences) with EXACTLY these keys:
{{
  "slug": "<kebab-case, max 40 chars>",
  "slide_count": {total},
  "title_card": {{
    "headline": "<punchy, lead with value, max 65 chars>",
    "subhead":  "<one compelling hook sentence, max 90 chars>"
  }},
  "content_slides": [
    {{
      "heading":     "<ALL CAPS, 2-5 words>",
      "body":        "<start each bullet with an em-dash —, 2-3 concrete points, each on its own line>",
      "image_query": "<2-4 word Pexels/Unsplash search phrase that visually fits this slide>"
    }}
  ],
  "outro_card": {{
    "cta":    "<single clear action, max 65 chars>",
    "handle": "{handle}"
  }},
  "caption":    "<{pname} caption: {prule}, no hashtags>",
  "hashtags":   {tags},
  "dm_keyword": "<one word>"
}}

RULES:
- content_slides MUST have EXACTLY {n_content} items.
- Every slide leads with value first, not background context.
- CTA is ONE action only (follow / DM / save / share).
- The copy, caption, and CTA MUST fit the selected platform and content type above.
{loc}- image_query must be a Pexels-compatible phrase (e.g. "city skyline night").
- Body bullets use em-dash format: — point one\n— point two"#,
        name = brand.name,
        profile = profile(brand),
        voice = voice(brand),
        tone = tone_block(tone),
        manual = if req.manual { MANUAL_RULES } else { "" },
    );
    let v = ai.chat_json(&system, &source(req))?;
    let mut plan = Plan::from_json(&v).map_err(StudioError)?;
    plan.format = "carousel".into();
    plan.platform = pkey.into();
    plan.content_type = ckey.into();
    plan.origin = if req.manual { "manual".into() } else { "rss".into() };
    if !tone.is_empty() {
        plan.tone = tone.into();
    }
    if req.polish {
        plan = polish(ai, brand, &req.story, plan, tone);
    }
    plan.content_slides.truncate(n_content);
    finish(plan, brand, &req.story)
}

/// A single-card plan (square / story / x / quote).
pub fn plan_single(ai: &Backend, brand: &Brand, req: &Request, fmt: &str) -> Result<Plan> {
    let note = match fmt {
        "story" => {
            "A vertical 1080x1920 Instagram/Facebook story. One headline, 1-2 short lines, and a 'cta' field with a 2-4 word tap action (e.g. 'DM us GROW')."
        }
        "x" => {
            "A landscape 1600x900 image for X/Twitter. Headline + 2-3 short bullets. Also write 'tweet_text': the actual post text under 270 chars, with 1-2 hashtags inline."
        }
        "quote" => {
            "A 1080x1350 quote card. Put one memorable line (a real quote from the source, or the post's sharpest takeaway — never an invented quote attributed to a real person) in 'quote' and who said it (or the brand) in 'subhead'."
        }
        _ => "A single square (1080x1080) Instagram feed post. One bold headline plus 2-3 em-dash bullet points. Punchy and self-contained.",
    };
    let extra = match fmt {
        "story" => "  \"cta\": \"<2-4 word tap action>\",\n",
        "x" => "  \"tweet_text\": \"<the X post text, under 270 chars, 1-2 inline hashtags>\",\n",
        "quote" => "  \"quote\": \"<the quote, max 160 chars>\",\n  \"subhead\": \"<attribution>\",\n",
        _ => "",
    };
    let tone = if req.tone.trim().is_empty() { brand.tone.trim() } else { req.tone.trim() };
    let loc = if brand.location.is_empty() { String::new() } else { format!("- Make {} relevance explicit when it is not obvious.\n", brand.location) };
    let system = format!(
        r#"You are a content planner for {name}. You write clear, value-first
social posts — no fluff.

CREATOR PROFILE:
{profile}
{voice}
{tone}
FORMAT: {note}
{manual}
Return ONE valid JSON object (no markdown, no code fences) with EXACTLY these keys:
{{
  "slug": "<kebab-case, max 40 chars>",
  "format": "{fmt}",
  "headline": "<punchy, value-first, max 70 chars>",
  "body": "<2-3 points, each starting with an em-dash, each on its own line>",
  "image_query": "<2-4 word Pexels/Unsplash phrase that fits this post>",
{extra}  "caption": "<platform caption, 2-4 sentences, no hashtags>",
  "hashtags": {tags},
  "dm_keyword": "<one word>"
}}

RULES:
- Lead with value, not background.
- One clear CTA only.
{loc}- Do not invent facts beyond the source story."#,
        name = brand.name,
        profile = profile(brand),
        voice = voice(brand),
        tone = tone_block(tone),
        manual = if req.manual { MANUAL_RULES } else { "" },
        tags = json!(brand.hashtags),
    );
    let v = ai.chat_json(&system, &source(req))?;
    let mut plan = Plan::from_json(&v).map_err(StudioError)?;
    plan.format = fmt.into();
    plan.platform = platform(&req.platform).0.into();
    plan.content_type = content(&req.content_type).0.into();
    plan.origin = if req.manual { "manual".into() } else { "rss".into() };
    if req.polish {
        plan = polish(ai, brand, &req.story, plan, tone);
    }
    finish(plan, brand, &req.story)
}

fn finish(mut plan: Plan, brand: &Brand, story: &Story) -> Result<Plan> {
    if plan.source_url.is_empty() {
        plan.source_url = story.url.clone();
    }
    if plan.source_image.is_empty() {
        plan.source_image = story.image.clone();
    }
    plan.normalize(brand);
    Ok(plan)
}

const COPY_KEYS: &[&str] = &[
    "headline",
    "subhead",
    "heading",
    "body",
    "caption",
    "cta",
    "tweet_text",
    "hook",
    "take",
    "points",
    "quote",
    "context",
    "source_label",
    "verdict",
    "banner",
];

/// Copy only text fields from `edited` into `base` (a rewrite may never change structure or facts
/// such as hashtags, image queries or the slug).
pub fn merge_copy(base: &Value, edited: &Value) -> Value {
    match (base, edited) {
        (Value::Object(b), Value::Object(e)) => {
            let mut out = b.clone();
            for (k, old) in b {
                let Some(new) = e.get(k) else { continue };
                match (old, new) {
                    (Value::String(_), Value::String(n)) if COPY_KEYS.contains(&k.as_str()) && !n.trim().is_empty() => {
                        out.insert(k.clone(), Value::String(n.trim().to_string()));
                    }
                    (Value::Object(_) | Value::Array(_), _) => {
                        out.insert(k.clone(), merge_copy(old, new));
                    }
                    _ => {}
                }
            }
            Value::Object(out)
        }
        (Value::Array(b), Value::Array(e)) => {
            Value::Array(b.iter().enumerate().map(|(i, o)| e.get(i).map_or_else(|| o.clone(), |n| merge_copy(o, n))).collect())
        }
        _ => base.clone(),
    }
}

/// The brand-voice copy pass. Never fails the plan: on any error the plan is returned as is.
pub fn polish(ai: &Backend, brand: &Brand, story: &Story, plan: Plan, tone: &str) -> Plan {
    let personality = brand.personality.trim();
    if personality.is_empty() {
        return plan;
    }
    let tone_line = if tone.is_empty() { String::new() } else { format!("\nREQUESTED TONE:\n{tone}\n") };
    let system = format!(
        "You are the final copy editor for {}.\nRewrite the post copy so it sounds unmistakably on-brand, while preserving the\nexisting JSON shape and all source facts.\n\nBRAND PERSONALITY:\n{personality}\n{tone_line}\nReturn ONE valid JSON object with the same structure as the input plan.\n\nEDITING RULES:\n- Rewrite only copy fields: headlines, subheads, headings, body text, captions, CTA, hooks, takes, verdicts, quotes, banners, and tweet text.\n- Do not change facts, counts, ranks, formats, URLs, image_query, hashtags, slug, handles, or dm_keyword.\n- Add personality through sharper framing, more specific stakes, and stronger audience relevance.\n- Keep copy concise enough for social graphics.\n- Do not add claims not supported by the source story.",
        brand.name
    );
    let base = plan.to_json();
    let user = format!(
        "Source story:\nTitle: {}\nSummary: {}\nURL: {}\n\nCurrent plan JSON:\n{}",
        story.title,
        photocraft_press::text::clip(&story.summary, 900),
        story.url,
        base
    );
    match ai.chat_json(&system, &user) {
        Ok(edited) => Plan::from_json(&merge_copy(&base, &edited)).unwrap_or(plan),
        Err(e) => {
            log::warn!("personality polish skipped: {e}");
            plan
        }
    }
}

/// Rewrite just the caption and hashtags.
pub fn regen_caption(ai: &Backend, brand: &Brand, plan: &Plan) -> Result<(String, Vec<String>)> {
    let (_, pname, prule, ntags, _) = platform(&plan.platform);
    let system = format!(
        "You write {pname} captions for {}.\nCREATOR PROFILE:\n{}\n{}\nReturn ONLY JSON: {{\"caption\": \"<{prule}, no hashtags>\", \"hashtags\": [<up to {ntags} relevant hashtags without #>]}}",
        brand.name,
        profile(brand),
        voice(brand)
    );
    let v = ai.chat_json(&system, &format!("Post plan:\n{}", plan.to_json()))?;
    let caption = v.get("caption").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if caption.is_empty() {
        return Err(StudioError("the AI returned no caption".into()));
    }
    let tags = match v.get("hashtags") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(|t| t.trim_start_matches('#').to_string()).collect(),
        Some(Value::String(s)) => photocraft_press::text::split_tags(s),
        _ => plan.hashtags.clone(),
    };
    Ok((caption, tags))
}

/// Alternative angles for a manual idea (Create ▸ Suggest angles).
pub fn suggest_angles(ai: &Backend, brand: &Brand, idea: &str, notes: &str) -> Result<Vec<String>> {
    let system = format!(
        "You help {} plan social posts.\nCREATOR PROFILE:\n{}\nSuggest 5 distinct, specific angles for a post on the user's idea, using only the idea and notes given. Return ONLY JSON: {{\"angles\": [\"<one-line angle>\", …]}}",
        brand.name,
        profile(brand)
    );
    let v = ai.chat_json(&system, &format!("Idea: {idea}\nNotes: {}", photocraft_press::text::clip(notes, 900)))?;
    Ok(v.get("angles").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default())
}

fn score_system(brand: &Brand) -> String {
    let mut s = format!(
        "You are a content relevance judge for {}, which posts Instagram content.\n\nCREATOR PROFILE:\n{}\n\nScore the story 0-100 for how well it fits as a post for this brand and its audience. Reward stories that map onto the niche AND can be turned into an engaging post; penalise off-topic material.",
        brand.name,
        profile(brand)
    );
    if !brand.scoring_guidance.trim().is_empty() {
        s.push_str(&format!("\n\nBRAND SCORING GUIDANCE:\n{}", brand.scoring_guidance.trim()));
    }
    s
}

/// Score stories in batches of 10 (one request each); stories a batch reply misses get 50.
/// Returns them best first, near-duplicates dropped.
pub fn score(ai: &Backend, brand: &Brand, stories: Vec<Story>, cancel: &dyn Fn() -> bool) -> Result<Vec<Story>> {
    let mut out: Vec<Story> = Vec::with_capacity(stories.len());
    for chunk in stories.chunks(10) {
        if cancel() {
            break;
        }
        let list: Vec<String> =
            chunk.iter().enumerate().map(|(i, s)| format!("[{i}] Title: {}\nSummary: {}", s.title, photocraft_press::text::clip(&s.summary, 400))).collect();
        let system = format!(
            "{}\n\nReturn ONLY valid JSON: {{\"results\": [{{\"index\": <int>, \"score\": <int 0-100>, \"reason\": \"<one sentence>\"}}]}}",
            score_system(brand)
        );
        let user = format!("Score each story (indexed 0-{}):\n\n{}", chunk.len().saturating_sub(1), list.join("\n\n"));
        let reply = ai.chat_json(&system, &user)?;
        let results: Vec<Value> = match &reply {
            Value::Array(a) => a.clone(),
            v => v.get("results").and_then(Value::as_array).cloned().unwrap_or_default(),
        };
        for (i, s) in chunk.iter().enumerate() {
            let r = results.iter().find(|r| r.get("index").and_then(Value::as_u64) == Some(i as u64));
            let mut s = s.clone();
            s.score = Some(r.and_then(|r| r.get("score")).and_then(Value::as_i64).unwrap_or(50).clamp(0, 100));
            s.reason = r.and_then(|r| r.get("reason")).and_then(Value::as_str).unwrap_or("batch reply incomplete").to_string();
            out.push(s);
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.score));
    let texts: Vec<String> = out.iter().map(|s| format!("{} {}", s.title, photocraft_press::text::clip(&s.summary, 300))).collect();
    let keep = photocraft_press::text::dedupe(&texts);
    Ok(keep.into_iter().filter_map(|i| out.get(i).cloned()).collect())
}
