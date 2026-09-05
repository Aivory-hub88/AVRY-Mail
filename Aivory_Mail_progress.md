# Aivory Mail — Progress, Config & Routing

**Repo:** https://github.com/Aivory-hub88/AVRY-Mail
**Branch:** `main` — VPS `ubuntu@129.226.155.216:63222` (`~/.ssh/claude_code_vps`) `aivory-network` `traefik:v2.11`
**Domain:** `mail.aivory.uk` (Cloudflare `aivory.uk` zone `518089ea559912f70a3a2911d0cf09af`)

## 1. Deploy & Infra

- **VPS:** Tencent `129.226.155.216` (`22` blocked, `63222` ssh, `avry-postgres:5432` `aivory_mail` `aivory:AivoryApp2026!@#123`, `JWT_SECRET=be44f...`, `INTERNAL_TOKEN=5367...`)
- **Images:** `avry-mail:8095` (Rust `bookworm` `lettre 0.11.23` `mail-send 0.4.9`), `avry-mail-web:3005` (Next.js), `avry-mail-smtp:2525/2587` (inbound ingress)
- **Compose:** `docker-compose.mail-prod.yml` (healthcheck `curl /health`, `traefik` `Host(mail.aivory.uk) && (PathPrefix(/v1/)||/health||/mcp)`)
- **Build fixes:** `Dockerfile:2` `FROM rust:bookworm`, `tower-http` `RequestBodyLimitLayer 100MB` + `DefaultBodyLimit::disable()` (`main.rs:69`), `mail-builder 0.3` + `rustls aws-lc-rs` `CryptoProvider` (`main.rs:16`).
- **Deploy recipe:** `git fetch && git reset --hard origin/main` on the VPS, `docker compose -f docker-compose.mail-prod.yml build <service>` (Rust build ~2-8min, backgrounded), then `up -d <service>`. Both `avry-mail` and `avry-mail-web` have their own Dockerfiles; only rebuild the one whose source actually changed.

## 2. Env (~/AVRY-Mail/.env, hybrid)

```
PORT=8095
MAIL_MODE=hybrid
DATABASE_URL=postgresql://aivory:AivoryApp2026!@123@avry-postgres:5432/aivory_mail
STORAGE_BACKEND=local STORAGE_PATH=/app/data/mail-storage
JWT_SECRET=be44f... INTERNAL_TOKEN=5367...
MAIL_ADMIN_EMAIL=admin@aivory.id MAIL_ADMIN_PASSWORD=Avry786876!@
SUPERADMIN_EMAIL=irfan.reichmann@aivory.uk
CORS_ORIGINS=https://mail.aivory.uk,https://aivory.id,https://dashboard.aivory.id
MAIL_MX_HOST=mail.aivory.uk SPF_INCLUDE_HOST=_spf.aivory.uk DMARC_REPORT_ADDRESS=dmarc@aivory.uk
CF_API_TOKEN=cfut_... CF_ACCOUNT_ID=5cc9dc6b811810f1867de0d48c411b0b CF_ZONE_ID=518089ea...
PUBLIC_API_URL=https://mail.aivory.uk   # used to rewrite inbound cid: image refs into real URLs — see §7
```

`docker-compose.mail-prod.yml` on the VPS is **not** committed to the repo (gitignored) — it's the one file to hand-edit directly on the VPS when a new env var like `PUBLIC_API_URL` needs adding to `avry-mail`'s `environment:` block.

## 3. DNS (Cloudflare, proxied:false for mail)

- `A mail.aivory.uk 129.226.155.216` `proxied:false` (`39afc637`) — sebelumnya `proxied:true` -> `104.21.15.183` bikin `MX` fallback `_dc-mx`
- `MX aivory.uk 10 mail.aivory.uk.` (`cd28c85c`) — `dig @arely MX` sekarang `10 mail.aivory.uk.` (sebelum `_dc-mx.ad458...`)
- `TXT aivory.uk v=spf1 include:_spf.aivory.uk ~all`, `TXT _spf.aivory.uk v=spf1 ip4:129.226.155.216 include:spf.mailersend.net include:relay.mailchannels.net ~all` (`c6d2d...`)
- `TXT aivory._domainkey.aivory.uk v=DKIM1; k=rsa; p=MIIBIjAN...`
- `TXT _dmarc.aivory.uk v=DMARC1; p=quarantine; rua=mailto:dmarc@aivory.uk` (`63b5d02`, sebelumnya `postmaster@`)
- Cloudflare Email Sending auto-provisions its own MX/SPF/DKIM/DMARC on a `cf-bounce.aivory.uk` subdomain — separate from Email Routing's records above, don't confuse the two when debugging deliverability.
- Verified via mail-tester.com: SPF/DKIM/DMARC all pass on outbound sent through Cloudflare.

## 4. DB & Migrations

- `mailboxes` 4: `hello@aivory.uk a44b...`, `irfan.reichmann@aivory.uk d163...`, `career@aivory.uk 6c40...`, `hello@demo.aivory.uk c2f9...` (`is_catch_all` `hello@aivory.uk=true`), plus `password_hash TEXT` (salted SHA-256, `sha256$<iter>$<salt>$<hash>`, 100k iterations — see `aivory-mail-core::password`)
- `domains` 2: `aivory.uk 6da68... Active`, `demo.aivory.uk 6a900... Active`, plus `admin_email TEXT` (`aivory.uk` → `irfan.reichmann@aivory.uk`) — see §6 auth
- `mailbox_aliases` `TEXT` (`domain_id`, `mailbox_id` `TEXT` vs `UUID` -> join `::uuid` di `routing.rs:50`); `send_as_aliases` also exists (distinct table — outbound "send as" identities, not inbound routing)
- `messages` (`headers_json JSONB`, `snoozed_until`), `threads`, `attachments` via `object_store` `local`
- `mail_filters`, `webhooks`, `webhook_deliveries`, `agent_tasks`, `contacts`, `groups`/`group_members`, `api_keys`, `audit_logs`, `ai_chat_history`, `mission_control_notifications`, etc.
- **Known schema drift**: several tables were migration-declared as `UUID`/`TIMESTAMPTZ`/`BOOLEAN` but are actually `TEXT`/`INTEGER` in the live Postgres DB (`groups.id`, `groups.created_at`, `send_as_aliases.*`). Binding a typed Rust value against the wrong live column type fails with `operator does not exist: text = uuid` — the recurring root cause of several 500s fixed this session (`groups.rs`, `send_as.rs`: bind `.to_string()` everywhere, read columns as `String`). Check the *live* schema with `\d <table>` before assuming migration files are accurate.
- SQLite dev schema (`main.rs::ensure_schema`) needs a column added to **both** the `CREATE TABLE IF NOT EXISTS` literal **and** the `alters` Vec — `alters` run before the `CREATE` on a fresh DB, so a column present only in `alters` never lands on a new install.

## 5. Routing — 3-Phase Parity

**`crates/aivory-mail-api/src/mail/routing.rs:23` `resolve_recipient(to)`**
1. `lower(address)=$1` exact `mailboxes`
2. `mailbox_aliases` `ma.mailbox_id::uuid=m.id` + `ma.domain_id::uuid=d.id`
3. `send_as_aliases` join fallback (Postgres `::text` cast + SQLite) — lets a message addressed to a configured send-as identity still land in the owning mailbox
4. `use_all_domains !=0` (`INTEGER` vs `BOOLEAN`)
5. `is_catch_all=true` (`hello@aivory.uk=true`)

**`inbound.rs:31` `handle_inbound_raw_with_folder`**
- `probe = Sent/Drafts ? from : to` -> `resolve_recipient(probe)` + fallback
- `forced_folder` skip `apply_filters`; else `FilterAction::Reject|Block|Forward|Move`
- `snippet` `chars().take(160)` fix `£`/`🚀` (multi-byte-safe char-count truncation, not byte-offset — a byte-offset slice used to panic the whole inbound pipeline when a multi-byte char straddled the cutoff)
- **Inline `cid:` image rewrite**: a sender's `<img src="cid:...">` (logo in an email signature, etc.) is unresolvable by any browser — it's not a real URL scheme. Attachment IDs are now pre-generated before `body_html` is stored, and every `cid:<content-id>` reference is rewritten to `{PUBLIC_API_URL}/v1/messages/{msg_id}/attachments/{att_id}` at ingest time (both the live inbound path and the Zoho-import path in `import_mail.rs`). `download_attachment` serves images with `Content-Disposition: inline` (not `attachment`) so they actually render instead of forcing a download / showing a broken-image icon.
- `50MB` limit

**`api/mod.rs` `GET /v1/stats?mailbox_id=`** per-mailbox `by_folder`; omitting `mailbox_id` returns instance-wide counts and now requires domain-admin auth (see §6). `web/page.tsx` fetches per-mailbox + clears stale `msgs`/`threads` state on mailbox/folder switch (see §8 for the cross-mailbox delete-leak bug this fixed).

## 6. API & Auth

- `POST /v1/auth/login` — `{email, password}`. Priority: a mailbox's own `password_hash` (if set) wins exclusively; falls back to `MAIL_ADMIN_PASSWORD`/`SUPERADMIN_EMAIL`/inspection-mode only when the mailbox has **no** own password set. Once an account has a real password, the shared admin password stops working for it — even if that address happens to equal the admin/superadmin email.
- `GET /v1/auth/me` — resolves the bearer JWT to `{email, mailbox_id, address, display_name, is_admin}`. `is_admin` now comes from `authz::is_admin` (see below), not just a hardcoded env-var comparison.
- **Domain-admin authorization** (`crates/aivory-mail-api/src/api/authz.rs`): each domain has exactly one admin mailbox, `domains.admin_email` (for `aivory.uk` that's `irfan.reichmann@aivory.uk`) — the only account allowed into the admin console and allowed to read/manage mailboxes other than its own. Falls back to the instance-wide `MAIL_ADMIN_EMAIL`/`SUPERADMIN_EMAIL` for ops access when no domain has an admin assigned. Enforced via an axum `route_layer` middleware (`authz::require_admin_mw`) wrapping a dedicated `admin_router()` group: `/v1/domains*`, `/v1/mailboxes*`, `/v1/audit-logs`, `/v1/groups*`, `/v1/api-keys*`, `/v1/mcp/generate-link`, `/v1/webhooks` (the registry CRUD, not the inbound webhook receivers). `/v1/stats` additionally checks `authz::require_admin` inline when `mailbox_id` is omitted.
  - **Before this**: every one of those endpoints had *zero* server-side check — an anonymous `curl` got the full mailbox list, could create/delete accounts, and could read any domain's DKIM private key. The admin frontend's own gate was "is there a token in `localStorage`", and none of its fetches even attached one.
  - `web/app/admin/page.tsx` now calls `/v1/auth/me` on mount and redirects non-admins to `/`, and every fetch goes through an `authFetch()` helper that attaches `Authorization: Bearer <token>`.
  - **Still open**: this covers the admin-console surface only. Ordinary mailbox-scoped endpoints (`/v1/messages`, `/v1/threads`, `/v1/ai/ask`, etc.) still trust a client-supplied `mailbox_id` without verifying it against the caller's own JWT identity — a full JWT-auth middleware across the *entire* API (not just the admin group) is the next real hardening step if/when prioritized.
- `GET /v1/messages?folder=...&mailbox_id=` `GET /v1/threads` `GET /v1/stats?mailbox_id=`
- `POST /v1/send` `{from,to,subject,text,html,attachments}` -> `outbound.rs` `2MB` `10x10MB` `DKIM` `send_email` — see §7 for the detached-task fix and the Cloudflare-attachments fix.
- `POST /v1/webhooks/inbound` `POST /v1/webhooks/cloudflare` `{"from","to","raw":base64,"folder"}` `50MB`
- `POST /v1/ai/ask` `GET /v1/ai/history` `POST /v1/ai/push-to-mission-control` — now mailbox-isolated end to end (see §8).

## 7. Outbound (`outbound.rs`)

**Cloudflare Email Sending is now the primary transport** (previously it silently 500'd on every attempt and every send fell through to SMTP without any visible symptom):
- `send_via_cloudflare` → `POST https://api.cloudflare.com/client/v4/accounts/{CF_ACCOUNT_ID}/email/sending/send` (the earlier `/zones/{zone_id}/...` URL doesn't exist for this API at all — that's what caused `email.sending.error.invalid_request_schema` on every call). Payload now also translates `req.attachments` into Cloudflare's own `{content, filename, type, disposition}` shape — this was missing entirely, so every attachment silently vanished on any send that went out via Cloudflare, with the API still reporting success.
- On Cloudflare failure, falls back through `send_via_fallback_chain`: `worker-http -> mailchannels -> smtp (mail_send)`.
- `send_via_mail_send` (`mail_send::SmtpClientBuilder` `smtp.mailersend.net:587`) is the last-resort fallback, no longer the primary path.
- **Detached send**: `/v1/send` and thread replies used to `.await` the full transport round-trip (2-4s) on the same HTTP connection the browser held open — closing the tab or navigating away mid-send aborted the request, and the message could reach the recipient but never get written to Sent. Both call sites now wrap `outbound::send_email(...)` in `tokio::spawn` and await the `JoinHandle` — dropping a `JoinHandle` does not cancel the spawned task, so a dead client connection can no longer cancel a send that already left the building.
- **Sent-message attachments now actually stored**: `store_sent_message` previously only set the `has_attachments` flag; the file itself was never written to object storage or given an `attachments` row, so a Sent message showing a paperclip had nothing behind it. `send_email()` now stores each attachment the same way inbound does, immediately after `store_sent_message`.
- Size limit: Cloudflare Email Sending caps total message size (incl. attachments) at **5 MiB** — over that, it errors and the fallback chain takes over automatically.

## 8. Frontend (`web/app/page.tsx`, `web/components/*`)

- `Inbox`/`Sent`/`Drafts`/`Snoozed`/`Archive`/`Spam`/`Trash`, `mailbox` selector `selectedMailboxId`
- **Cross-mailbox delete-leak fix**: `bulkDelete`/`bulkMove`/`bulkMarkRead` computed `isThreadView` as `conversationView && activeFolder==="Inbox" && !search && threads.length>0` — when a mailbox switch (or folder switch) left the Inbox genuinely empty, `threads.length===0` made this fall through to a **stale `msgs` array** from whatever was previously loaded (a different mailbox/folder), so a "Delete N message(s)?" confirm could reference and delete messages that don't even belong to the currently-viewed account. Fixed by dropping the `threads.length>0` clause (the mode should follow which folder/view is active, not how many rows happen to be in it) and clearing `msgs` whenever entering conversation-view Inbox.
- **`MailBody.tsx`**: received HTML is sanitized with DOMPurify and rendered in a sandboxed `<iframe srcDoc>` (previously raw `dangerouslySetInnerHTML` with zero sanitization). Explicit `color-scheme:light` (not `light dark`) + pinned white background — dark OS theme previously flipped the iframe background to black while text stayed dark-on-light, producing unreadable dark-on-black for real transactional emails. Height auto-sizes via `ResizeObserver`, not a one-shot `load`-event measurement (a bad first measurement while the iframe's own width was still 0 used to freeze the height at ~2000px of blank space).
- **`ComposeModal.tsx`**: rich-text compose now uses a real `contentEditable` + `document.execCommand` instead of wrapping selections in raw `**`/`<b>` markers inside a `<textarea>` (which just printed the literal markup — a textarea can never render bold/italic). Toolbar: font family/size, insert-photo (inline `<img>` data URI), emoji picker, link, strikethrough. Switching from plain-text into rich mode remounts the `contentEditable` (loses any live selection), so the very first formatting action bakes the requested tag directly into the HTML the remount renders with; subsequent actions run `execCommand` against the live DOM selection normally. A `useEffect` keyed on the remount explicitly re-focuses and collapses the caret to the end — otherwise the browser defaulted the caret to the very start of the content after a remount, making new keystrokes appear to type backwards.
- **AI Assistant isolation**: `fetch_overview`/`fetch_message_context`/`fetch_thread_context`/`fetch_thread_memory` (`ai_chat.rs`) previously queried with no `mailbox_id` filter at all — every account got the exact same instance-wide numbers and could see context from other mailboxes' messages/threads. All four now require and filter by `mailbox_id`, with ownership checks on any client-supplied message/thread ID. The assistant's own label was also "zeroclaw vanilla" (the underlying runtime's internal name) — now "Aivory Mail Assistant" everywhere user-facing.
- **Mobile responsive layout** (Gmail-app style, `md:` breakpoint): sidebar becomes a slide-in drawer behind a hamburger button instead of a persistent 280px column; list/detail no longer render side-by-side below `md` — selecting a message/thread hides the list and shows the detail full-screen with a back arrow (reuses the existing `isNoSplit` fixed-fullscreen pattern, made unconditional on narrow viewports instead of gated behind the desktop reading-pane setting); a Compose FAB (bottom-right) replaces the sidebar's Compose button, which is behind the drawer on mobile; the Ask AI Assistant floating panel is desktop-only (its screen corner is the Compose FAB's on mobile) with a dedicated drawer entry point instead, opening full-screen; message rows show a sender-initial avatar circle in place of the desktop bulk-select checkbox.
- **Accent color**: iterated per feedback away from teal (`#005a5e`) to a warm neutral currently `#ccc1a8` / hover `#ada48f` (same pairing pattern each iteration) across `page.tsx`, `ComposeModal`, `AskAIAssistant`, `admin`, `login`, `calendar`, `settings*`, and the rendered-email link color in `MailBody`.
- `AskAIAssistant.tsx` floating `fixed bottom-6 right-6` (desktop) / drawer-triggered full-screen (mobile, see above).
- Global focus styling (`globals.css`): text inputs/textareas/selects/`[contenteditable]` get no visible focus outline at all — just the native caret plus a faint `rgba(0,0,0,0.035)` background darkening (Gmail-style); other focusable elements keep a `2px solid var(--primary)` outline for keyboard-nav accessibility.

## 9. Worker (`worker/worker.js`, `wrangler.jsonc`)

- `email(message, env, ctx)` forward `POST ${AIVORY_MAIL_API_URL}/v1/webhooks/cloudflare` (`x-internal-token`)
- `fetch /send` outbound via `SEND_EMAIL` + fallback `mailchannels` — largely superseded by `send_via_cloudflare` (Email Sending REST API) as the primary transport now, see §7
- Deploy via `PUT /accounts/5cc9dc6b.../workers/scripts/aivory-mail-worker` (bypass `wrangler` `memberships`), routes `mail.aivory.uk/send*` + `worker.aivory.uk/*` (`c7e332...`, `7081b0...`)

## 10. Migration Zoho -> AVRY

- EML import via `crates/aivory-mail-api/src/bin/import_mail.rs` (standalone binary) — idempotent via Message-ID dedup, uses the same `send_as_aliases` mailbox-resolution fallback and the same `cid:` image rewrite as live inbound (§5, §7).

## 11. Next / TODO

- Full JWT-auth middleware across the *entire* API, not just the admin-console group (§6) — ordinary endpoints still trust a client-supplied `mailbox_id` without verifying it belongs to the caller.
- `page`/`per_page` query params are read correctly in `messages::list` but the same care hasn't been audited across every other `Query<serde_json::Value>` handler — worth a pass if a folder with very large message counts is reported as not paging.
- No rate limiting on `/v1/auth/login`, `/v1/send`, or `/v1/webhooks/inbound` — brute-force/spam-send abuse protection not yet implemented.
- `build_message` `html` + `attachments` `multipart` via `lettre` can still hit `InvalidContentType` for complex cases — works for `text/plain` via `mail_send` and for the common HTML+inline-image case exercised so far.
- Historical messages ingested *before* the `cid:` rewrite fix (§5/§7) still have dead `cid:` links in their stored `body_html` — not backfilled, since attachment insertion order isn't reliably recoverable well enough to safely re-map cid → attachment after the fact. New/future mail is unaffected.
