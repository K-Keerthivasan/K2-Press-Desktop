# Email campaigns: the K2 Digital Media playbook

How K2 Digital Media makes and sends client email campaigns (first client: Phoenix Fine Touch).
**Anyone who drafts or sends for a client follows this file: people, Claude and ChatGPT.** If a
step can't be followed, stop and ask. Never work around it.

## The flow

| # | Step | Who | Output |
|---|---|---|---|
| 1 | **Brief.** Client gives the topic, offer, dates, audience segment | Client → K2 | Brief in `library/campaigns/<client>/<yyyy-mm>_<slug>/brief.md` |
| 2 | **Copy.** Subject, preheader, headline, body, CTA in the client's voice (`personality` in `config.yaml`) | K2 (AI-assisted) | `copy.md` |
| 3 | **Design.** Post Studio › client › Post › **Make all versions**. This renders the email header and `email.html` in the client's look, plus the matching social posts | K2 | `outputs/<client>/<date>_<slug>_versions/` |
| 4 | **Internal check.** Use the checklist below | K2 | Ticked checklist |
| 5 | **Client review.** Send the client a **preview link or PDF/screenshot**, never the list. Edit until the client approves in writing | Client | Written approval (email/message), saved with the campaign |
| 6 | **Test send.** To K2 and client test inboxes only (Gmail, Outlook, phone). Check links, unsubscribe, images, spam score | K2 | Test results |
| 7 | **Send.** From the email service (ESP), one message per recipient, to the approved segment only | K2 | Campaign report |
| 8 | **After.** Unsubscribes synced to the suppression list within 24 h (the law allows 10 business days; we're faster). Report opens/clicks/unsubscribes to the client | K2 | `report.md` |

Social posts for the same campaign use the same plan, so they come out in the same frame and words.

## Privacy rules (non-negotiable)

These exist because an earlier email was sent with everyone in **CC**, so every recipient saw
every other address. That must never happen again.

1. **Never CC or BCC a list.** Every email is sent one-to-one by the ESP. No Gmail/Outlook
   group sends, not even with BCC: one wrong field exposes everyone.
2. **The list lives in one place only**, the ESP (or an encrypted export the client hands over).
   Never in this repo, `config.yaml`, the K2 Press folder, chats, AI prompts, screenshots or
   review documents.
3. **No AI tool ever sees the contact list.** Claude and ChatGPT write copy and templates with
   merge tags (`{{first_name}}`, `{{unsubscribe_url}}`) and never get real names or addresses.
4. **Client review gets content, not contacts.** Show segment size ("approx. 420 recipients")
   and never a sample of addresses.
5. **No personal data in links.** Unsubscribe and tracking links carry an opaque token, never
   an email address or name.
6. **Least access.** Only K2 staff who send campaigns get ESP access, with 2-factor login. Client
   lists imported from files: delete the file after import.
7. **Breach plan.** If a list or address leaks: stop sending, tell the client the same day, work
   out who was affected, and follow the client's privacy obligations (PIPEDA: report to the
   Privacy Commissioner and notify people when there's a real risk of significant harm).

## Compliance (Canada: CASL)

Every commercial email must have:

- **Consent.** Express (they opted in), or implied (customer in the last 2 years / enquiry in the
  last 6 months). Keep **who, when, how** consent was given for each contact (the ESP stores it).
  No purchased or scraped lists.
- **Sender identified.** The business name, plus **a mailing address** and a phone, email or web
  contact. The template prints `name · address` in the footer.
- **Unsubscribe that works.** One click, no login. Honoured promptly (law: within 10 business
  days; ours: within 24 h). It must keep working for at least 60 days after the send.
- **Honest subject and sender.** No misleading subject lines, and "From" is the client's name.

## The email template (`email.html`)

Made by **Make all versions** (`crates/studio/src/email.rs`), 600 px, table-based, inline styles
for Gmail and Outlook:

- Hidden **preheader** (inbox preview text).
- Header image in the client's look, headline, gold rule, sub line, body, checklist, CTA button.
- Client contact bar (location · phone · website).
- **Compliance footer:** why you're getting this, the sender's name and mailing address, and an
  **Unsubscribe** button.
- **Agency credit:** small grey "Campaigns by K2 Digital Media" linking to
  `https://campaigns.k2digitalmedia.ca`. It's deliberately quiet: the client's brand leads.

Set per client in `config.yaml`:

```yaml
campaign:
  reason: "You are receiving this email because you are a Phoenix Fine Touch client or asked to hear from us."
  unsubscribe_url: "{{unsubscribe_url}}"   # merge tag the ESP replaces per recipient
  credit: "Campaigns by K2 Digital Media"   # "" hides it
  credit_url: "https://campaigns.k2digitalmedia.ca"
  preheader: ""
```

The HTML never contains a recipient. Change `{{unsubscribe_url}}` to the ESP's own tag when you
import it (Mailchimp `*|UNSUB|*`, Brevo `{{ unsubscribe }}`, MailerLite `{$unsubscribe}`). Host
the header image on https (ESP image library or B2/Supabase storage), because local paths don't
work in inboxes.

## How the pieces fit (current setup)

| Piece | Where it runs | Notes |
|---|---|---|
| Sending, lists, templates | **Listmonk** (Docker on the laptop, `127.0.0.1:9000`) | One-to-one sends via each client's SMTP (Phoenix: Workspace mailbox) |
| Workspaces, drafts, videos, reports | **K2 Campaign Hub** (`127.0.0.1:19080`, Tailscale for phone) | Never sends; final send happens in Listmonk |
| Graphics (headers, posts) | **K2 Press Desktop**, Service look | Images only; the email text stays live HTML |
| Video files | **Backblaze B2, private bucket** | Lifecycle deletes each video after its 30/60-day retention |
| Video relay | Cloudflare Worker (`campaigns.k2digitalmedia.ca`, K2's own) | Hands out short-lived signed B2 links; recipients never see a page there |
| Unsubscribe | **Supabase** `email-unsubscribe` function (Shorts Factory project) | Always on, independent of the laptop |

### Video in the email

- **Apple Mail, Samsung Mail:** the real MP4 plays inside the email (poster, controls, sound on tap).
- **Gmail, Outlook, others:** the looping GIF preview (first frame = designed banner).
- **Images off:** the text panel with the offer and contact link.
- Every click (GIF, panel link) goes to the client's own website. Nothing links to a K2 page.
- The `<video>` source is `campaigns.k2digitalmedia.ca/media/videos/<id>`. Each play is redirected
  to a short-lived, file-scoped download link from the private B2 bucket. After the retention
  date the relay answers **410 Gone** and B2 deletes the file; the player then shows the poster
  and the GIF address serves the static banner.
- Hub: *Video library › upload* (30 or 60 days), then in the campaign editor choose
  **"Video plays in the email"** (the default).

### Unsubscribe

- Footer link: `https://<supabase>/functions/v1/email-unsubscribe/u/<client>/{{ .Campaign.UUID }}/{{ .Subscriber.UUID }}`.
  Only Listmonk's random UUIDs, never an email address.
- The page asks once ("Unsubscribe from Phoenix Fine Touch emails?") and records it on one press.
  It's a POST, so mail scanners that open links don't unsubscribe people. RFC 8058 one-click POSTs
  are accepted too.
- Supabase table `email_unsubscribes` (RLS on, no public access) holds `client, campaign UUID,
  subscriber UUID, time`.
- The Campaign Hub pulls new rows every 15 minutes and on demand (`POST /api/unsubscribes/sync`).
  It unsubscribes that person from **that client's lists only** in Listmonk and marks the row synced.
  **Run a sync right before every send.**
- Config: `EMAIL_UNSUB_URL` + `EMAIL_SYNC_SECRET` in `campaign-hub/.env`. The same secret is a
  Supabase function secret.
- Still to do: open/click tracking needs a public Listmonk root URL. Sending domains need
  **SPF, DKIM and DMARC** (Phoenix: Google DKIM key pending).

## Pre-send checklist

- [ ] Client approved this exact version in writing (saved with the campaign)
- [ ] Segment = approved audience, consent recorded, suppression list applied
- [ ] Sent one-to-one from the ESP (no CC/BCC, no mail client)
- [ ] Subject, preheader and From name are honest and approved
- [ ] Footer: sender name + mailing address + reason + working Unsubscribe
- [ ] All links work (CTA, website, phone `tel:`, unsubscribe), with UTM tags on CTA links
- [ ] Header image hosted on https, alt text set, looks right in Gmail, Outlook and on mobile
- [ ] Test sends received and checked; spam score acceptable
- [ ] No personal data anywhere outside the ESP (repo, prompts, screenshots, review docs)

## Risks and what we do about them

| Risk | Impact | Prevention |
|---|---|---|
| Addresses exposed (CC/BCC, shared screenshot, list in a doc) | Client trust, PIPEDA breach | One-to-one ESP sends only; list never leaves the ESP; review shows counts not contacts |
| Sending without consent | CASL penalties (up to $10M for organisations), complaints | Consent recorded per contact; no bought/scraped lists; implied consent expiry tracked |
| Broken or missing unsubscribe | CASL breach, spam complaints, blocklisting | Footer button + List-Unsubscribe headers; tested on every test send |
| Spam folder / domain reputation | Low reach, blocked domain | SPF/DKIM/DMARC, warm up gradually, clean bounces, <0.3% complaints |
| Wrong version sent / not approved | Client relationship | Written approval saved; send only the approved file |
| Wrong segment | Irrelevant mail, unsubscribes | Segment named in the approval; double-check count before send |
| AI writes off-brand or makes claims | Reputation | Client `personality` rules; human review; client review |
| AI tool given personal data | Data leak to a third party | Rule 3: AI never gets contacts, only merge tags |
| Images unavailable | Broken email | Host images on https; alt text; test send |

## Choosing the email service

Pick one ESP for K2's clients (one account, a sub-account or audience per client):

| Option | Good for | Notes |
|---|---|---|
| Mailchimp | Easiest, client-friendly reports | Built-in unsubscribe/consent; costs more as lists grow |
| Brevo | Cheap, good EU/CA privacy tooling | Transactional + campaigns; API for automation |
| MailerLite | Simple, cheap | Good templates and landing pages |
| Amazon SES + our own pages | Lowest cost, full control | We build unsubscribe/suppression ourselves (Supabase) |

Recommendation: start on a hosted ESP (Mailchimp or Brevo) so consent, unsubscribe and
suppression are handled from day one. Import `email.html` as a custom template.
