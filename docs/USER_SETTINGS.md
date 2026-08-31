# Aivory Mail — User Settings

Gmail / Zoho / Outlook parity settings, exposed at `/settings/mail` in the web
UI and backed by the `user_settings` KV table (migration `006_settings.sql`).

## Storage model

**`user_settings`** — key/value per category (optionally per mailbox):

```
id           TEXT PRIMARY KEY
tenant_id    TEXT (default 'default')
mailbox_id   TEXT NULL          — NULL = applies to all mailboxes
category     TEXT               — general | inbox | compose | appearance |
                                  notifications | shortcuts | storage | forwarding
key          TEXT
value        TEXT               — stored as string, cast on read
updated_at   TEXT
UNIQUE(tenant_id, mailbox_id, category, key)
```

The API **seeds defaults** per category if nothing is stored yet
(`settings.rs::default_for`), so `GET /v1/settings?category=X` always returns
a complete object.

Related tables (migration `006_settings.sql`):

| Table                  | Purpose                                     | API                        |
|------------------------|---------------------------------------------|----------------------------|
| `mail_filters`         | rules: `criteria_json`, `action_json`, enabled | `/v1/filters`            |
| `mail_labels`          | labels with color                           | `/v1/labels`               |
| `vacation_responders`  | out-of-office, per mailbox, start/end, interval | `/v1/vacation`          |
| `send_as_aliases`      | send-as aliases per mailbox (schema ready)  | —                          |
| `forwarding_rules`     | forward-to + keep_copy per mailbox (schema ready) | —                    |

## Categories & keys

| Category        | Key                           | Default        | Options / notes                          |
|-----------------|-------------------------------|----------------|------------------------------------------|
| **general**     | `undo_send_seconds`           | `10`           | 5 / 10 / 20 / 30 s                       |
|                 | `density`                     | `comfortable`  | comfortable / compact / cozy             |
|                 | `conversation_view`           | `true`         | bool                                     |
|                 | `page_size`                   | `20`           | 20 / 50 / 100                            |
|                 | `language`                    | `en`           | (schema)                                 |
| **inbox**       | `inbox_type`                  | `Default`      | Default / Unread first / Starred / Priority Inbox |
|                 | `categories`                  | `Primary,Promotions,Social` | comma list           |
| **compose**     | `default_font`                | `Manrope`      | Manrope / Verdana / Arial                |
|                 | `font_size`                   | `14`           | 12 / 14 / 16                             |
|                 | `font_color`                  | `#111827`      | hex                                      |
|                 | `compose_format`              | `html`         | html / text                              |
|                 | `always_show_cc`              | `false`        | bool                                     |
|                 | `always_show_bcc`             | `false`        | bool                                     |
|                 | `always_show_from`            | `false`        | bool                                     |
|                 | `outbox_delay_minutes`        | `0`            | 0 / 1 / 2 / 5 min                        |
| **appearance**  | `theme`                       | `light`        | light / dark                             |
|                 | `density`                     | `comfortable`  | comfortable / compact                    |
|                 | `reading_pane`                | `right`        | right / bottom / no-split                |
| **notifications**| `desktop_sound`              | `true`         | bool                                     |
|                 | `new_mail_banner`             | `true`         | bool                                     |
|                 | `email_notifications`         | `all`          | (schema)                                 |
| **shortcuts**   | `enabled`                     | `true`         | bool                                     |
|                 | `custom`                      | `{}`           | JSON map c → compose, e → archive, r → reply, / → search |
| **storage**     | `days_to_sync`                | `30`           | 7 / 30 / 90                              |
|                 | `auto_archive_days`           | `0`            | 0 = off                                  |
|                 | `download_attachments_wifi_only` | `true`      | bool                                     |
| **forwarding**  | `forward_to`                  | ``             | destination address                      |
|                 | `keep_copy`                   | `true`         | bool                                     |
## UI — `/settings/mail`

Ten tabs (left sidebar on desktop, horizontal chips on mobile):

| Tab                | Label                       |
|--------------------|-----------------------------|
| `general`          | General                     |
| `inbox`            | Inbox                       |
| `signatures`       | Signatures                  |
| `compose`          | Compose                     |
| `filters`          | Filters & Labels            |
| `forwarding`       | Forwarding & POP/IMAP       |
| `appearance`       | Appearance                  |
| `notifications`    | Notifications               |
| `shortcuts`        | Shortcuts                   |
| `storage`          | Storage & Offline           |

The full settings UI is wired to `/v1/settings` — every control calls
`POST /v1/settings` on change and reloads the category.

## Parity reference (research summary)

**Gmail** — General (undo send 5–30 s, density, conversation view, max page
size, stars, nudges, smart compose), Inbox type (Default/Important-first/
Unread-first/Starred/Priority), Categories, Filters & Blocked, Forwarding &
POP/IMAP, Labels, Themes & display.

**Zoho** — most granular for business: System & Appearance (startup view,
mails per page, font, 12/24 h time, language, external images), Compose
(rich/text editor, reply headers, undo send, outbox delay 1–120 m, font
family/size/color, UTF-8 encoding, auto-add recipients, Cc/Bcc auto-hide),
Signatures (multiple per alias + default per From), Vacation reply (duration +
interval), Filters, Archive, Send Mail As, Storage, Themes, Shortcuts.

**Outlook** — Message options (after move/delete, notifications, sound, empty
deleted on sign-out, missing-attachment warn), Read receipts, Conversations
sorting, Signature + auto-include, Message format HTML/plain + always show
Bcc/From, Message list + reading pane (mark read timer), Focused/Other,
categories, reactions, quick steps, delay send, recall, offline.

## Status

Two levels of "done" matter here: whether the setting is stored/exposed
(Schema/API/UI), and whether the app actually **applies** it. The two used to
diverge a lot — settings saved to the KV table with nothing reading them
back. As of the Phase 2 pass, the five items the KV-only gap mattered most
for are now real: undo send, density, conversation view, filters, vacation
auto-reply, and send-as.

| Feature                        | Schema | API       | UI        | Actually applied? |
|--------------------------------|--------|-----------|-----------|--------------------|
| General — undo send            | ✅     | ✅        | ✅        | ✅ delays the real `/v1/send` call (`ComposeModal.tsx`) |
| General — density              | ✅     | ✅        | ✅        | ✅ affects message-row padding (`page.tsx`) |
| General — conversation view    | ✅     | ✅        | ✅        | ✅ switches inbox to thread grouping via `/v1/threads` |
| General — page size            | ✅     | ✅        | ✅        | ✅ used in `/v1/messages?per_page=` |
| Inbox type + categories        | ✅     | ✅        | ✅        | ⏳ stored, not yet applied to list ordering |
| Compose (font/cc/bcc/delay)    | ✅     | ✅        | ✅        | ⏳ cc/bcc/font stored, not yet applied; outbox delay superseded by undo send |
| Appearance (theme/pane)        | ✅     | ✅        | ✅        | ⏳ stored, theme/pane not yet applied |
| Notifications                  | ✅     | ✅        | ✅        | ⏳ stored, not yet wired to real notifications |
| Shortcuts                      | ✅     | ✅        | ✅        | ⏳ stored, keys not yet bound |
| Storage & Offline              | ✅     | ✅        | ✅        | ⏳ stored, no offline cache yet |
| Signatures                     | ✅ (002)| ✅       | ✅ (inbox modal) | ✅ applied to compose |
| Filters                        | ✅     | ✅        | ✅        | ✅ routes inbound mail to the matched folder (`inbound.rs`) |
| Labels                         | ✅     | ✅        | ✅        | ⏳ stored, not yet attached to messages |
| Vacation responder             | ✅     | ✅        | ✅ (bound to a real mailbox) | ✅ auto-replies once per sender per `interval_days` |
| Forwarding / POP / IMAP        | ✅     | ✅ (KV)   | ✅ (KV)   | ⏳ stored, forwarding not yet executed |
| Send-as aliases                | ✅     | ✅ `/v1/send-as` | ✅ (Forwarding & Send As tab + compose From dropdown) | ✅ selectable as `from`; still gated on that domain being verified |
| Forwarding rules table         | ✅     | ⏳        | ⏳ (uses KV) | — out of scope for this pass |

## Example

```bash
# Read general settings (seeded with defaults)
curl -s 'http://localhost:8095/v1/settings?category=general' | jq .data

# Change undo-send to 30s
curl -s -X POST http://localhost:8095/v1/settings \
  -H 'content-type: application/json' \
  -d '{"category":"general","key":"undo_send_seconds","value":"30"}'

# Vacation responder (per mailbox)
curl -s -X POST http://localhost:8095/v1/vacation \
  -H 'content-type: application/json' \
  -d '{"mailbox_id":"<uuid>","enabled":true,"subject":"Out of office","body":"Back on Monday."}'
```
|                 | `pop_enabled`                 | `false`        | bool                                     |
|                 | `imap_enabled`                | `true`         | bool                                     |