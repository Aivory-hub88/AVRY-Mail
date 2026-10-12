# Aivory Mail — API Reference

Base URL: `http://localhost:8095` (dev) / `https://mail.aivory.uk` (prod).

Envelope: `{ "success": bool, "data": … }`, errors `{ "success": false, "error": … }`.

## Health & Stats

| Method | Path          | Description                          |
|--------|---------------|--------------------------------------|
| GET    | `/health`     | Health check (`status/version/mode/storage/db`) |
| GET    | `/v1/health`  | Alias of `/health`                   |
| GET    | `/v1/stats`   | Counts: domains, mailboxes, messages |

## Domains

Custom-domain onboarding is Zoho/Google-Workspace style: add a few DNS
records at your existing registrar (no nameserver migration). Every domain
gets a verification token + an RSA-2048 DKIM keypair generated immediately
on creation.

| Method | Path                    | Description                          |
|--------|-------------------------|--------------------------------------|
| GET    | `/v1/domains`           | List domains                         |
| POST   | `/v1/domains`           | Create domain `{domain}` — generates verification token + DKIM keypair |
| GET    | `/v1/domains/:id`       | Domain detail (incl. `failure_reason` if verification failed) |
| DELETE | `/v1/domains/:id`       | Delete domain                        |
| POST   | `/v1/domains/:id/verify`| Real DNS TXT lookup against `_aivory-verify.<domain>` — sets `Active` on match, else keeps `Pending` + `failure_reason` |
| GET    | `/v1/domains/:id/dns`   | Full DNS checklist (MX/SPF/DKIM/DMARC/verification), each with live status `Missing/Correct/Mismatch` — works for any domain, not just Cloudflare-hosted ones |
| GET    | `/v1/domains/:id/dkim`  | Just the DKIM TXT record (public key only, for copy-paste) |

A domain must be `Active` (verified) with a DKIM key on file before `POST
/v1/send` will accept mail `from` an address on it — see [Send](#send).

## Mailboxes

| Method | Path                | Description                                    |
|--------|---------------------|------------------------------------------------|
| GET    | `/v1/mailboxes`     | List (`?domain_id=`)                           |
| POST   | `/v1/mailboxes`     | Create with optional web-login secret (min 8 chars, never touches IMAP) |
| GET    | `/v1/mailboxes/:id` | Detail                                         |
| PUT    | `/v1/mailboxes/:id` | Update display_name and/or secret - web-login only |
| POST   | `/v1/mailboxes/:id/imap-password` | Issue or rotate IMAP+SMTP credential, empty body = auto-generate, shown once |
| DELETE | `/v1/mailboxes/:id/imap-password` | Revoke mail-client access, web login keeps working |
| DELETE | `/v1/mailboxes/:id` | Delete                                         |

## Profile & Avatar

| Method | Path              | Description                                                        |
|--------|-------------------|--------------------------------------------------------------------|
| GET    | `/v1/me/profile`  | Own profile `{email, mailbox_id, address, display_name, has_avatar, avatar_url}` (`?mailbox_id=` admin view-as) |
| PUT    | `/v1/me/profile`  | Update own display name `{mailbox_id?, display_name}` (≤120 chars, empty clears) |
| GET    | `/v1/me/avatar`   | Avatar bytes (`?mailbox_id=` + `?token=` for `<img>`, `?v=` cache-buster). 404 when none set |
| POST   | `/v1/me/avatar`   | Upload multipart field `avatar` (PNG/JPG/GIF/WebP by magic bytes, ≤2 MiB) → stored `avatars/<mailbox_id>.<ext>` |
| DELETE | `/v1/me/avatar`   | Remove avatar (file + metadata)                                    |
| GET    | `/v1/avatars`     | Batch sender→avatar lookup `?emails=a,b` (≤100) → `{email: {mailbox_id, avatar_url}}` for instance mailboxes with avatar |

## Messages & Threads

| Method | Path                                            | Description                         |
|--------|-------------------------------------------------|-------------------------------------|
| GET    | `/v1/messages`                                   | List. Query: `folder`(default Inbox), `mailbox_id`, `search`, `page`, `per_page` (≤100), `category` (Inbox tab, see below) |
| GET    | `/v1/messages/:id`                               | Get one (marks read)                |
| DELETE | `/v1/messages/:id`                               | Delete                             |
| PUT    | `/v1/messages/:id/read`                          | `{is_read: bool}`                   |
| POST   | `/v1/messages/:id/move`                          | `{folder}` — allowed: Inbox, Sent, Drafts, Spam, Trash, Archive |
| GET    | `/v1/messages/:id/attachments/:att_id`           | Download attachment (binary)        |
| POST   | `/v1/messages/:id/star`                          | Toggle star                         |
| POST   | `/v1/messages/:id/share`                         | Create share link (7d JWT)          |
| GET    | `/v1/threads`                                    | List threads. `category` = Inbox tab (a conversation's tab is its newest Inbox message's) |
| GET    | `/v1/threads/:id`                                | Thread + messages                   |
| POST   | `/v1/threads/:id/reply`                          | Reply to thread                     |
| GET    | `/v1/threads/:id/crawl`                          | Crawl thread context                |
| GET/POST | `/v1/threads/:id/follow-up`                    | Follow-up suggestion / trigger      |
| GET    | `/v1/threads/:id/memory`                         | Budgeted thread memory for LLM      |
| GET    | `/v1/drafts`                                     | List drafts (folder=Drafts)         |
| POST   | `/v1/drafts`                                     | Save/update draft                   |

### Inbox tabs

Every new Inbox message gets a `category`: `primary`, `promotions`, `social`,
`updates` or `forums`. It is decided from the sender and headers only
(`List-Unsubscribe`, `Precedence`, `List-Id`/`List-Post`, `Auto-Submitted`,
ESP headers, noreply-style senders, social-network domains), never by sending
content to a model. **Mail with no sign of being machine-sent stays in
Primary.** A user's per-sender choice overrides the rules.

The tabs shown come from the `inbox.categories` setting (default
`Primary,Promotions,Social`); mail in a hidden tab is listed under Primary.
Promotions and Social don't trigger push notifications. Mail from before tabs
existed has `category = NULL` and counts as Primary until backfilled.

| Method | Path | Description |
|--------|------|-------------|
| GET  | `/v1/categories?mailbox_id=` | `{tabs: [{id, unread}]}` — enabled tabs with unread Inbox counts |
| POST | `/v1/messages/:id/category` | `{category, apply_to_sender?=true}` — move the whole conversation; with `apply_to_sender` the sender is locked to that tab (future mail + their other Inbox mail) |
| POST | `/v1/categories/backfill?mailbox_id=` | Classify up to 500 uncategorized Inbox messages; repeat while `remaining > 0` |

## Shared messages

| Method | Path           | Description                                 |
|--------|----------------|---------------------------------------------|
| GET    | `/v1/share/:id`| Get shared message. Query `?t=<token>` (JWT) |

## Send

| Method | Path              | Description                                      |
|--------|-------------------|--------------------------------------------------|
| POST   | `/v1/send`        | Send `{from, to[], cc[], bcc[], subject, text/html, attachments}` → `{id, status:"queued"}`. Rejects if the `from` domain isn't `Active`/verified. Every outbound message is DKIM-signed with the sending domain's key before delivery. |
| POST   | `/v1/send/batch`  | Batch ≤50 messages `{messages: [...]}`. Every entry's `from` is authorized before any is sent; one foreign address → `403`, nothing sent. |

**Who may send as `from`.** The caller's own mailbox address, one of their
own Send As aliases, or anything for an instance admin. Anything else → `403`.
An invalid payload (bad address, no body) → `400`.

**`Idempotency-Key` header (optional, `/v1/send` and `/v1/threads/:id/reply`).**
8–200 characters of `A-Z a-z 0-9 . _ : -` (a UUID works). Retrying with the
same key returns the first result instead of sending twice. Keys are per user.

| Situation | Response |
|-----------|----------|
| First use, or retry after the first one **failed** | sent normally |
| Retry after the first one succeeded | `202` with the **original** `id`; nothing is sent |
| Same key, different message | `409` `{code: "idempotency_conflict"}` |
| Same key while the first is still sending | `409` `{code: "send_in_progress"}` |
| First request never finished (>5 min) | `409` `{code: "send_outcome_unknown"}` — check Sent before retrying with a new key |
| Malformed key | `400` |

The web composer sends one key per message, keeps it when the network fails
(so a retry can't double-send), and drops it once the server answers.

## Telegram notifications

New Inbox mail can be forwarded to the owner's Telegram chat. One bot serves
the whole server (`TELEGRAM_BOT_TOKEN`); each mailbox stores its own chat in
`user_settings`, category `notifications`:

| Key | Meaning |
|-----|---------|
| `telegram_chat_id` | Numeric chat id (negative for a group). `@handles` are refused so the shared bot can't message arbitrary channels. |
| `telegram_enabled` | `"false"` switches it off; anything else is on. |
| `telegram_preview` | `"true"` adds the first lines of the body. Default off: only sender and subject leave the server. |

Promotions and Social mail never notify. The message is HTML-escaped, spaced to
one per ~1.1 s per chat, and dropped if a chat is flooded; Telegram failures
never affect delivery. The bot token is stripped from logged errors.

| Method | Path | Description |
|--------|------|-------------|
| GET  | `/v1/telegram/status` | `{configured, bot_username}` — whether this server has a bot, and its @name |
| POST | `/v1/telegram/test` | `{mailbox_id}` — send a test message to that mailbox's saved chat. Maps Telegram errors to plain messages (chat not found / bot blocked / bad token); one test per mailbox per 10 s |

## Intelligence & AI

| Method | Path                      | Description                                   |
|--------|---------------------------|-----------------------------------------------|
| POST   | `/v1/intelligence/analyze`| Heuristic + optional AI gateway analyze       |
| POST   | `/v1/intelligence/suggest`| Suggest actions/replies                       |
| POST   | `/v1/agent/actions`       | Dispatch agent action                         |
| GET    | `/v1/knowledge/compile`   | Auto-compile agent knowledge per tenant/scope |
| POST   | `/v1/ai/draft-reply`      | `{thread_id \| message_id, hint?}` → `{draft, model}` — see below |

### Draft reply

Writes a reply to a conversation for the owner to edit; nothing is sent or
saved. Same access rules as reading the conversation (another mailbox's
thread → `404`). The prompt (`aivory_mail_core::drafting`) gets the last 8
non-draft messages (quoted history stripped, hidden unicode removed, 12k-char
budget, newest kept) fenced as **untrusted data**, plus the owner's
`ai.drafting_instructions` setting and optional one-off `hint` as trusted
input. Rules: body only (no subject/signature/placeholders), the other
person's language, never invent prices/dates/commitments/attachments.

Called directly on OpenRouter (`OPENROUTER_API_KEY`, `MAIL_INTELLIGENCE_MODEL`;
`OPENROUTER_BASE_URL` overrides the endpoint), never via the tool-using agent
gateway. No key → `503 {code: "ai_not_configured"}`; model failure →
`502 {code: "ai_unavailable"}`. There is no canned fallback text.

## MCP (Remote)

| Method | Path              | Description                                     |
|--------|-------------------|--------------------------------------------------|
| GET    | `/v1/mcp/tools`   | List MCP tools                                  |
| GET    | `/mcp`            | MCP tools (GET)                                 |
| POST   | `/mcp`            | MCP JSON-RPC: `initialize`, `tools/list`, `tools/call` |
| POST   | `/v1/mcp/generate-link` | Generate MCP connection URL `{name | key_id}` |

Tools exposed by MCP: `search_mail`, `get_inbox_overview`, `get_thread_memory`,
`get_knowledge_compile`, `draft.create` (scope `mail.draft.create`, no
confirmation — pairs with `send_mail` for review-then-send loops), `send_mail`.

**Agent send safety** (all MCP sends: `send_mail`, `reply_mail`, `forward_mail`):

- `reply_mail` refuses with `conversation_advanced` (plus the newest message's
  id) when the thread has a newer non-draft message than the one being
  answered — on the preview and again on the confirmed call. Reply to the
  newest message instead.
- `AGENT_DAILY_SEND_LIMIT` (default `100`, `0` = no cap) — agent sends per
  mailbox per UTC day; over it the tool answers `daily_send_limit` and sends
  nothing. Web-app sends are never counted.
- `AGENT_SEND_ENABLED=false` — stops every agent send at once and hides the
  send tools from `tools/list`; calls answer `agent_send_disabled`.

## Search

| Method | Path                 | Description                           |
|--------|----------------------|---------------------------------------|
| GET    | `/v1/search`         | Hybrid search (vector+FTS) `?q=`      |
| GET    | `/v1/inbox/overview` | 1-call inbox stats                    |
## Calendar

| Method | Path                          | Description                              |
|--------|-------------------------------|------------------------------------------|
| GET    | `/v1/calendar/status`         | Calendar status (linked providers)       |
| GET    | `/v1/calendar/event-types`    | Booking event types                      |
| GET    | `/v1/calendar/slots`          | Available slots                          |
| POST   | `/v1/calendar/bookings`       | Create booking                           |
| POST   | `/v1/calendar/propose`        | Propose slots for a thread/message       |
| GET    | `/v1/calendar/events`         | List events (Google-parity CRUD)         |
| POST   | `/v1/calendar/events`         | Create event `{title, start_at, end_at, …}` |
| PUT    | `/v1/calendar/events/:id`     | Update `{title, start_at, end_at, calendar}` |
| DELETE | `/v1/calendar/events/:id`     | Delete event                             |

## Webhooks (inbound)

| Method | Path                      | Description                                    |
|--------|---------------------------|------------------------------------------------|
| POST   | `/v1/webhooks/inbound`    | Generic inbound (JSON or raw MIME base64)      |
| POST   | `/v1/webhooks/cloudflare` | Cloudflare Email Routing worker payload        |

Both webhooks are callable only by the mail ingress, not by users: send the
server's `INTERNAL_TOKEN` in the `x-internal-token` header or the request is
rejected with `401`. They bypass the session login (the SMTP ingress and the
Cloudflare worker have none), so this token is the whole defence — anyone who
could call them unauthenticated could plant mail in any mailbox and pick its
folder. A Bearer session token is not accepted here.

`INBOUND_WEBHOOK_AUTH=warn` is a rollout switch only: bad callers are accepted
and logged (`ACCEPTED WITHOUT A VALID TOKEN`). Unset, or any other value,
enforces.

## Signatures

| Method | Path              | Description                                    |
|--------|-------------------|------------------------------------------------|
| GET    | `/v1/signatures`  | List (`?mailbox_id=`)                          |
| POST   | `/v1/signatures`  | Create `{mailbox_id, name, html, text, is_default}` |
| PUT    | `/v1/signatures/:id` | Update `{name, html, text, is_default}`     |
| DELETE | `/v1/signatures/:id` | Delete                                      |

## User settings (Gmail/Zoho/Outlook parity)

| Method | Path                 | Description                                    |
|--------|----------------------|------------------------------------------------|
| GET    | `/v1/settings`       | Get settings `?category=` (seeded with defaults) |
| POST   | `/v1/settings`       | Set `{category, key, value, mailbox_id?}`       |
| GET    | `/v1/labels`         | List labels                                    |
| POST   | `/v1/labels`         | Create label `{name, color}`                   |
| DELETE | `/v1/labels/:id`     | Delete label                                   |
| GET    | `/v1/filters`        | List filters/rules                             |
| POST   | `/v1/filters`        | Create filter `{name, criteria, action}` — `criteria` supports `{from\|subject\|body: "substring"}`, `action` supports `{move: "<folder>"}`. Enabled rules run against every inbound message (first match wins); applied in `inbound.rs` before the message is stored. |
| GET    | `/v1/vacation`       | Get vacation responder `?mailbox_id=` — auto-replies once per sender per `interval_days` while `enabled` |
| POST   | `/v1/vacation`       | Set vacation responder `{mailbox_id, enabled, subject, body}` |
| GET    | `/v1/send-as`        | List send-as aliases `?mailbox_id=` — users see only their own mailbox; admins any (or all) |
| POST   | `/v1/send-as`        | Create alias `{mailbox_id, alias_email, display_name, is_default}` — admin only |
| DELETE | `/v1/send-as/:id`    | Delete alias — admin only                      |

> Categories & defaults: see [USER_SETTINGS.md](./USER_SETTINGS.md).

## Integrations · Email Account (IMAP)

Self-service IMAP credential for every mailbox (was admin-only). `POST /v1/mailboxes/:id/imap-password` is still the admin path; the Settings UI now uses the user-facing flow below so every user can manage their own credential and admin can still check status.

| Method | Path                              | Auth | Description |
|--------|-----------------------------------|------|-------------|
| GET    | `/v1/integrations/email`          | JWT  | Own mailbox status — `{host, port, username, status, connected, has_password, last_tested_at}`. Never returns password. |
| POST   | `/v1/integrations/email/test`     | JWT  | Validate & TCP-probe `{host, port, username, password}` without saving. Returns `{reachable}` or error. |
| POST   | `/v1/integrations/email`          | JWT  | Save & connect `{host, port, username, password}` — hashes into `mailboxes.password_hash_dovecot` + upserts `email_integrations` row. |
| DELETE | `/v1/integrations/email`          | JWT  | Disconnect — clears `password_hash_dovecot`, marks `email_integrations.status=disconnected`. Web login keeps working. |
| GET    | `/v1/integrations/email/admin?mailbox_id=` | Admin | Check any mailbox status (host/port/connected, no password) — admin console uses this to answer "user lupa" without ever revealing the secret. |

UI: `Settings > Integrations > Email Account` — form `host/port/username/password` (masked, show/hide, Test before Save). After save only `Connected as user@domain.com` + Disconnect/Reconnect is shown; password is never rendered again (App-passwords parity).

## API Keys

| Method | Path                  | Description                                        |
|--------|-----------------------|----------------------------------------------------|
| GET    | `/v1/api-keys`        | List (auto-creates a default `avry-…` dev key)     |
| POST   | `/v1/api-keys`        | Create `{name}` → returns `key_raw` **once**       |
| DELETE | `/v1/api-keys/:id`    | Delete key                                         |

Keys are SHA-256 hashed for storage; the raw value is returned only at
creation and kept in `key_raw` so the UI can render a consistent masked +
reveal (`avry-…`).

## Realtime

| Method | Path                  | Description                          |
|--------|-----------------------|--------------------------------------|
| GET    | `/v1/realtime/ws`     | WebSocket (`?mailbox_id=`) push for new mail/updates |

## Cerveau Bridge (Mail ↔ Cerveau agents, per-mailbox isolated)

Zeroclaw vanilla tetap jadi fast path untuk Ask AI. Kalau butuh memory dalam, Mail relay ke Cerveau daemon dengan mailbox scope yang sama — Cerveau lalu callback ke Mail via MCP dengan `mailbox_id` yang sama, jadi tidak bocor.

| Method | Path              | Auth | Description |
|--------|-------------------|------|-------------|
| GET    | `/v1/cerveau/agents` | JWT | List entrypoints yang bisa di-delegate: `workflow_*`, `mail_ops`, `mail_memory` |
| POST   | `/v1/cerveau/ask`    | JWT (own mailbox) | Relay `{question, context:{mailbox_id?, thread_id?}, agent?}` ke `COGNEE_URL`/`AI_GATEWAY_URL` (Cerveau). Header `x-mailbox-id` + `x-internal-token` diteruskan, jawaban di-save sebagai `ai_chat_history` dengan `model=cerveau`. Forbidden jika mailbox_id ≠ own. Fallback ke heuristic jika daemon down. |

MCP Mail sekarang `mailbox_id`-aware: `search_mail` dan `get_inbox_overview` menerima `mailbox_id` opsional — bila diisi, query di-filter `WHERE mailbox_id=...` (Postgres/SQLite). Cerveau yang panggil via `POST /mcp` harus selalu sertakan `mailbox_id` dari relay agar tetap per-user.

## Cognee / Knowledge

| Method | Path              | Description                                  |
|--------|-------------------|----------------------------------------------|
| GET    | `/v1/cognee/sync` | Sync mailbox ingestion to cognee sidecar     |

## Internal (SMTP ingress only)

| Method | Path                          | Description                                    |
|--------|-------------------------------|-------------------------------------------------|
| GET    | `/v1/internal/resolve-recipient?to=` | `x-internal-token` protected. Called by `aivory-mail-smtp` at `RCPT TO` time so unknown mailboxes get a real `550 5.1.1 User unknown` instead of being accepted and stored under an orphaned tenant. Returns `{accept, mailbox_id, tenant_id, reason}`. |