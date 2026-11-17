//! The email version of a post: a table-based HTML newsletter (600 px, inline styles, works in
//! Gmail and Outlook) around the post's email header image, in the brand's colours.

use photocraft_press::{Brand, Plan};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `header_src` is the header image's URL or a path relative to the HTML file.
pub fn html(plan: &Plan, brand: &Brand, header_src: &str) -> String {
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
    let script = if brand.script_tagline.trim().is_empty() { String::new() } else { format!(r#"<p style="margin:0 0 8px;font:italic 20px Georgia,serif;color:{accent}">{}</p>"#, esc(brand.script_tagline.trim())) };
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title></head>
<body style="margin:0;padding:0;background:#f2f4f5">
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
</table></td></tr></table>
</body></html>
"#,
        title = esc(head),
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
