//! The email version of a post: a table-based HTML newsletter (600 px, inline styles, works in
//! Gmail and Outlook) around the post's email header image, in the brand's colours.
//!
//! Privacy and compliance (CASL): the HTML never holds a recipient. Each copy is personalised at
//! send time by replacing `{{unsubscribe_url}}` (and optionally `{{first_name}}`) per recipient,
//! one message per person, never CC/BCC lists. Every email carries the sender's name and mailing
//! address, why the reader gets it, a working unsubscribe button, and a quiet
//! "Campaigns by K2 Digital Media" credit.

use photocraft_press::{Brand, Plan};
use serde_json::Value;

/// Per-campaign footer settings. Defaults come from the brand's optional `campaign:` block in
/// `config.yaml` (`unsubscribe_url`, `reason`, `credit`, `credit_url`, `preheader`).
#[derive(Clone, Debug, PartialEq)]
pub struct Footer {
    /// Replaced per recipient by the sender (a merge tag, never a real address).
    pub unsubscribe_url: String,
    /// Why the reader receives this ("You are receiving this because ...").
    pub reason: String,
    /// The agency credit, shown small and grey at the very bottom.
    pub credit: String,
    pub credit_url: String,
    /// Inbox preview text (hidden in the body).
    pub preheader: String,
}

pub const UNSUBSCRIBE_TAG: &str = "{{unsubscribe_url}}";
pub const DEFAULT_CREDIT_URL: &str = "https://campaigns.k2digitalmedia.ca";

impl Footer {
    pub fn for_brand(brand: &Brand) -> Footer {
        let cfg = brand.extra.get("campaign").cloned().unwrap_or(Value::Null);
        let get = |k: &str| cfg.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let name = if brand.name.trim().is_empty() { "us".to_string() } else { brand.name.trim().to_string() };
        Footer {
            unsubscribe_url: get("unsubscribe_url").unwrap_or_else(|| UNSUBSCRIBE_TAG.into()),
            reason: get("reason").unwrap_or_else(|| format!("You are receiving this email because you are a client of, or asked to hear from, {name}.")),
            // `credit: ""` hides the credit; a missing key keeps it.
            credit: cfg.get("credit").and_then(Value::as_str).map_or_else(|| "Campaigns by K2 Digital Media".to_string(), |s| s.trim().to_string()),
            credit_url: get("credit_url").unwrap_or_else(|| DEFAULT_CREDIT_URL.into()),
            preheader: get("preheader").unwrap_or_default(),
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `header_src` is the header image's URL or a path relative to the HTML file.
pub fn html(plan: &Plan, brand: &Brand, header_src: &str) -> String {
    html_with(plan, brand, header_src, &Footer::for_brand(brand))
}

/// [`html`] with explicit footer settings.
pub fn html_with(plan: &Plan, brand: &Brand, header_src: &str, footer: &Footer) -> String {
    let accent = if brand.theme.accent.trim().is_empty() { "#0B5557" } else { brand.theme.accent.trim() };
    let gold = if brand.theme.accent2.trim().is_empty() { "#E4B039" } else { brand.theme.accent2.trim() };
    let text = if brand.theme.text.trim().is_empty() || brand.style != "service" { "#1F2933" } else { brand.theme.text.trim() };
    let head = [&plan.headline, &plan.title_card.headline].into_iter().find(|t| !t.trim().is_empty()).map_or("", |t| t.trim());
    let sub = [&plan.subhead, &plan.title_card.subhead].into_iter().find(|t| !t.trim().is_empty()).map_or("", |t| t.trim());
    let mut points: Vec<String> = photocraft_press::service::bullets(&plan.body);
    for s in &plan.content_slides {
        if !s.heading.trim().is_empty() {
            points.push(s.heading.trim().to_string());
        }
    }
    points.truncate(6);
    let caption = plan.caption.trim();
    let cta = [plan.outro_card.cta.as_str(), plan.cta.as_str()].into_iter().find(|t| !t.trim().is_empty()).unwrap_or("Get in touch");
    let link = if !brand.website.trim().is_empty() {
        let w = brand.website.trim();
        if w.starts_with("http") { w.to_string() } else { format!("https://{w}") }
    } else if !brand.phone.trim().is_empty() {
        format!("tel:{}", brand.phone.trim().replace([' ', '-'], ""))
    } else {
        "#".into()
    };
    let list: String = points.iter().map(|p| format!(r#"<tr><td style="padding:6px 0;font:16px/1.5 Arial,sans-serif;color:{text}"><span style="color:{accent};font-weight:bold">&#10003;</span>&nbsp; {}</td></tr>"#, esc(p))).collect();
    let contact: Vec<String> = [brand.location.trim(), brand.phone.trim(), brand.website.trim()].iter().filter(|s| !s.is_empty()).map(|s| esc(s)).collect();
    let sender = [brand.name.trim(), brand.address.trim()].iter().filter(|s| !s.is_empty()).map(|s| esc(s)).collect::<Vec<_>>().join(" &middot; ");
    let preheader = if footer.preheader.is_empty() { sub } else { footer.preheader.as_str() };
    let credit = if footer.credit.trim().is_empty() {
        String::new()
    } else {
        format!(
            r#"<tr><td align="center" style="padding:10px 24px 0;font:11px/1.5 Arial,sans-serif;color:#a3acb3"><a href="{}" style="color:#a3acb3;text-decoration:none">{}</a></td></tr>"#,
            esc(&footer.credit_url),
            esc(&footer.credit)
        )
    };
    let script = if brand.script_tagline.trim().is_empty() { String::new() } else { format!(r#"<p style="margin:0 0 8px;font:italic 20px Georgia,serif;color:{accent}">{}</p>"#, esc(brand.script_tagline.trim())) };
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title></head>
<body style="margin:0;padding:0;background:#f2f4f5">
<div style="display:none;max-height:0;overflow:hidden;opacity:0;color:transparent">{preheader}</div>
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f2f4f5"><tr><td align="center" style="padding:24px 12px">
<table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background:#ffffff;border-radius:12px;overflow:hidden">
<tr><td><img src="{img}" width="600" alt="{alt}" style="display:block;width:100%;height:auto;border:0"></td></tr>
<tr><td style="padding:28px 32px 8px">
{script}<h1 style="margin:0 0 10px;font:900 28px/1.15 Arial,sans-serif;color:{accent};text-transform:uppercase">{head}</h1>
<div style="width:90px;height:6px;background:{gold};border-radius:3px;margin:0 0 16px"></div>
<p style="margin:0 0 14px;font:bold 17px/1.4 Arial,sans-serif;color:{text}">{sub}</p>
<p style="margin:0 0 14px;font:16px/1.6 Arial,sans-serif;color:{text}">{caption}</p>
<table role="presentation" cellpadding="0" cellspacing="0">{list}</table>
</td></tr>
<tr><td align="center" style="padding:18px 32px 30px">
<a href="{link}" style="display:inline-block;background:{gold};color:{accent};font:bold 17px Arial,sans-serif;text-decoration:none;padding:14px 34px;border-radius:30px">{cta}</a>
</td></tr>
<tr><td style="background:{accent};padding:16px 24px;text-align:center;font:14px/1.6 Arial,sans-serif;color:#ffffff">{contact}<br><span style="opacity:.8">{name} &middot; {handle}</span></td></tr>
</table>
<table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%">
<tr><td align="center" style="padding:18px 24px 0;font:12px/1.6 Arial,sans-serif;color:#6b7680">{reason}<br>{sender}</td></tr>
<tr><td align="center" style="padding:12px 24px 0"><a href="{unsub}" style="display:inline-block;border:1px solid #c5ccd1;color:#4a555e;font:12px Arial,sans-serif;text-decoration:none;padding:7px 18px;border-radius:16px">Unsubscribe</a></td></tr>
{credit}
</table></td></tr></table>
</body></html>
"#,
        title = esc(head),
        preheader = esc(preheader),
        reason = esc(&footer.reason),
        unsub = esc(&footer.unsubscribe_url),
        img = esc(header_src),
        alt = esc(head),
        head = esc(head),
        sub = esc(sub),
        caption = esc(caption).replace('\n', "<br>"),
        cta = esc(cta),
        contact = contact.join(" &nbsp;|&nbsp; "),
        name = esc(&brand.name),
        handle = esc(&brand.at_handle()),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn email_has_unsubscribe_sender_and_quiet_credit_and_no_recipient() {
        let brand = Brand::from_value("phoenix", &json!({"name": "Phoenix Fine Touch", "address": "317 Adelaide St S, London, ON", "campaign": {"preheader": "A fresh start"}}));
        let plan = Plan::from_json(&json!({"headline": "Cleaner spaces. Better everyday."})).unwrap();
        let h = html(&plan, &brand, "email/01_email.png");
        assert!(h.contains(r#"href="{{unsubscribe_url}}""#), "per-recipient unsubscribe merge tag");
        assert!(h.contains("317 Adelaide St S, London, ON"), "sender mailing address");
        assert!(h.contains("Campaigns by K2 Digital Media") && h.contains(DEFAULT_CREDIT_URL));
        assert!(h.contains("A fresh start"));
        assert!(!h.contains("mailto:"), "no recipient addresses baked in");
        let custom = Brand::from_value("x", &json!({"name": "X", "campaign": {"unsubscribe_url": "https://campaign.example/u/{{token}}", "credit": ""}}));
        let h2 = html(&plan, &custom, "h.png");
        assert!(h2.contains("https://campaign.example/u/{{token}}") && !h2.contains("Campaigns by"));
    }
}

/// Writes a sample Phoenix email to `EMAIL_PREVIEW` (an .html path) for a visual check:
/// `EMAIL_PREVIEW=… EMAIL_HEADER=… cargo test -p photocraft-studio email_preview -- --ignored`.
#[cfg(test)]
#[test]
#[ignore]
fn email_preview() {
    #![allow(clippy::unwrap_used)]
    use serde_json::json;
    let Ok(out) = std::env::var("EMAIL_PREVIEW") else { return };
    let header = std::env::var("EMAIL_HEADER").unwrap_or_default();
    let brand = Brand::from_value(
        "phoenix",
        &json!({"name": "Phoenix Fine Touch", "handle": "@phoenixfinetouch", "style": "service", "script_tagline": "The fine touch. Every time.",
                "location": "London, Ontario & Surrounding Areas", "phone": "519-999-5000", "website": "phoenixfinetouch.ca",
                "address": "317 Adelaide St S Unit 113, London, ON N5Z 3L3",
                "theme": {"accent": "#0B5557", "accent2": "#E4B039", "text": "#123E40"},
                "campaign": {"reason": "You are receiving this email because you are a Phoenix Fine Touch client or asked to hear from us."}}),
    );
    let plan = Plan::from_json(&json!({
        "headline": "Cleaner spaces. Better everyday.", "subhead": "A little more care. A lot more attention to detail.",
        "body": "— Top to bottom detailed cleaning\n— Organic & eco-friendly options\n— Health, pet & children friendly\n— Fully insured & bonded",
        "caption": "Hi {{first_name}},\nFrom the comfort of home to the pace of business, our team brings the same care to every clean across London and surrounding areas.",
        "outro_card": {"cta": "Request your free quote"}}))
    .unwrap();
    std::fs::write(out, html(&plan, &brand, &header)).unwrap();
}
