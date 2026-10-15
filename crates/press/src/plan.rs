//! A post plan: the copy of one post, in the same JSON shape the K2 Press planner writes and its
//! library stores (`library/plans/<brand>/*.json`). Unknown keys are kept, so a plan read from the
//! earlier app and saved here loses nothing.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleCard {
    pub headline: String,
    pub subhead: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentSlide {
    pub heading: String,
    pub body: String,
    pub image_query: String,
    /// Local image for this slide (fetched or chosen); empty = none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutroCard {
    pub cta: String,
    pub handle: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plan {
    pub slug: String,
    /// `carousel`, `square`, `story`, `x`, `quote`, `cover`… (empty = carousel).
    pub format: String,
    pub title_card: TitleCard,
    pub content_slides: Vec<ContentSlide>,
    pub outro_card: OutroCard,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub dm_keyword: String,
    /// Single-card formats.
    pub headline: String,
    pub subhead: String,
    pub body: String,
    pub cta: String,
    pub tweet_text: String,
    pub quote: String,
    pub image_query: String,
    /// Cover / single-card background image (local path).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub image: String,
    pub platform: String,
    pub content_type: String,
    pub tone: String,
    pub origin: String,
    pub source_url: String,
    pub source_image: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Plan {
    /// Read any plan JSON; a malformed one is an error, not a silent empty plan.
    pub fn from_json(v: &Value) -> Result<Plan, String> {
        let mut p: Plan = serde_json::from_value(v.clone()).map_err(|e| format!("not a post plan: {e}"))?;
        // The planner sometimes writes hashtags as one string.
        if p.hashtags.is_empty()
            && let Some(Value::String(s)) = v.get("hashtags")
        {
            p.hashtags = crate::text::split_tags(s);
        }
        Ok(p)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn format(&self) -> crate::Format {
        crate::Format::from_key(&self.format)
    }

    /// The post's display title.
    pub fn title(&self) -> &str {
        [&self.title_card.headline, &self.headline, &self.quote, &self.slug].into_iter().find(|s| !s.trim().is_empty()).map_or("Untitled post", |s| s.as_str())
    }

    /// A fresh carousel with `content` empty slides, for a post started by hand.
    pub fn blank(brand: &crate::Brand, content: usize) -> Plan {
        Plan {
            format: "carousel".into(),
            title_card: TitleCard { headline: "Your headline".into(), subhead: "One compelling hook sentence.".into() },
            content_slides: (0..content.clamp(1, 8))
                .map(|i| ContentSlide { heading: format!("POINT {}", i + 1), body: "— First point\n— Second point".into(), ..Default::default() })
                .collect(),
            outro_card: OutroCard { cta: "Follow for more".into(), handle: brand.at_handle() },
            hashtags: brand.hashtags.clone(),
            platform: "instagram".into(),
            content_type: "general".into(),
            origin: "manual".into(),
            ..Default::default()
        }
    }

    /// Caption with hashtags appended, as published (Instagram allows 2,200 characters).
    pub fn full_caption(&self) -> String {
        let mut text = self.caption.trim().to_string();
        if !self.hashtags.is_empty() {
            let tags: Vec<String> = self
                .hashtags
                .iter()
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .map(|t| if t.starts_with('#') { t.to_string() } else { format!("#{t}") })
                .collect();
            if !tags.is_empty() {
                text = format!("{text}\n\n{}", tags.join(" ")).trim().to_string();
            }
        }
        if text.chars().count() > 2200 {
            text = text.chars().take(2199).collect::<String>().trim_end().to_string() + "…";
        }
        text
    }

    /// Clamp the slide list to the planner's limits (1–8 content slides) and fill the slug.
    pub fn normalize(&mut self, brand: &crate::Brand) {
        if self.content_slides.len() > 8 {
            self.content_slides.truncate(8);
        }
        if self.slug.trim().is_empty() {
            self.slug = crate::text::slug(self.title(), "post");
        }
        if self.outro_card.handle.trim().is_empty() {
            self.outro_card.handle = brand.at_handle();
        }
        if self.format.is_empty() {
            self.format = "carousel".into();
        }
    }
}
