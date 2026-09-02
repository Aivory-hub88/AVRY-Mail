# Aivory Mail — Complete Documentation

> **Business email, AI-powered, you own.** No per-message SaaS tax. Rust core + Next.js 15 + multi-environment (VPS / Cloudflare / Hybrid) + AI triage + Workflow triggers. Production ready since `5094074`.

**Repo:** https://github.com/Aivory-hub88/AVRY-Mail · **Live:** `https://mail.aivory.uk` (Proxied, Traefik `mail.aivory.uk`) — API `https://mail.aivory.uk/v1`, Web `https://mail.aivory.uk` (Next.js `:3005`), SMTP ingress `:2525` (VPS).

---

## 1. What it is

**Aivory Mail = Mailflare (Cloudflare-native inbox, 2.2k★) + Mailcow hardening + Zoho/Gmail business features + AI**, re-implemented in **Rust (Axum, SQLx, Tokio)** — no direct code copy (Mailflare license forbids commercial SaaS derivative).

| Mailflare | Mailcow | Zoho/Gmail | Aivory Mail adds |
|-----------|---------|------------|------------------|
| Cloudflare Email Routing only | Postfix/Dovecot full groupware | SaaS, ads, ecosystem lock | **SMTP ingress `:2525` + CF Routing + hybrid**, `DbPool` Postgres/SQLite + `ObjectStore` local/R2/S3, **AI gateway + MCP + n8n** (`MAIL_MODE=vps\|cloudflare\|hybrid`) |
| D1+R2 only | Docker 12 containers | Cloud infra | Tenant-aware (`tenant_id`), DKIM `aivory._domainkey`, SPF/DMARC checklist, **bulk actions**, **labels**, **forwarding**, **theme/pane**, **shortcuts** |

**Who it's for:** Teams with custom domains (agency, product, client workspaces) that want **owned inbox** + optional zero-ops Cloudflare inbound.

---

## 2. Architecture

```
          Aivory Identity (Supabase JWT, tenant)
                    │
            ┌───────▼────────┐
            │  Aivory Mail   │← login /admin
            └───────┬────────┘
      ┌─────────────┼──────────────┬──────────────┐
      │             │              │              │
 Cloudflare     Aivory DB     Aivory Files    Aivory Web
 Email Layer  (Postgres/   (R2/S3/local)   (Next 15 :3005)
               SQLite)       ▲
      │             │              │              │
      └─────────────┼──────────────┘              │
                    │                            │
            ┌───────▼────────┐                  │
            │  AI Gateway    │← Cerveau/ZeroClaw/OpenRouter
            └───────┬────────┘                  │
                    │                            │
            ┌───────▼────────┐                  │
            │ Aivory Workflow│← n8n-as-code :3500
            └────────────────┘
```

**Crates:** `aivory-mail-core` (types, `mail-parser`, `dns`, `filters`, `intelligence`), `aivory-mail-storage` (`DbPool`, `ObjectStore`), `aivory-mail-api` (`Axum`, `RealtimeHub`, `auth`, `groups`, `mail/*`), `aivory-mail-smtp` (VPS ingress `:2525` → `POST /v1/webhooks/inbound`), `web/app` (Next.js), `worker/worker.js` (CF `email()` → `/v1/webhooks/cloudflare`), `migrations/001…011`.

**Multi-env `MAIL_MODE`:**
| Mode | Inbound | Outbound | Storage | DB |
|------|---------|----------|---------|-----|
| `vps` | SMTP `:2525` → webhook | SMTP relay `lettre` DKIM `aivory._domainkey` + `mail-auth` signing | local / S3 | Postgres / SQLite |
| `cloudflare` | Email Routing → Worker `email()` → `/v1/webhooks/cloudflare` | Cloudflare Email Service (`CF_API_TOKEN` + `CF_ZONE_ID`) | R2 | D1 / Postgres |
| `hybrid` | both | CF first, fallback SMTP | R2/S3 | Postgres |

Inbound lifecycle (VPS): `SMTP RCPT TO` → `GET /v1/internal/resolve-recipient` (550 if no mailbox/catch-all) → `DATA` → `POST /v1/webhooks/inbound` → `mail-parser` → `resolve_recipient` backstop → `apply_filters` + `is_blocked` → `vacation` dedup + `maybe_forward` + `cognee` + `RealtimeHub` WS + `WORKFLOW_URL/webhook/email-received`.

Outbound: `POST /v1/send` → `require_verified_sender_domain` (must be `Active` + DKIM key) → build MIME → `dkim::sign` (`mail-auth`) → store + `send_via_cloudflare` else `send_via_smtp` (fail-closed in prod if `SMTP_HOST` missing) → `RealtimeHub`.

---

## 3. Features — Production (no hard-coded/placeholder)

### 3.1 Auth & Admin (paritas Zoho/Gmail Business)

| Area | Endpoint / UI | Status |
|------|---------------|--------|
| **Login** | `POST /v1/auth/login` JWT 7d (`mail_admin_email`/`mail_admin_password` env, default `Avry786876!@`, superadmin `irfan.reichmann@aivory.uk` + `INSPECTION_MODE=true` allow any email + correct password), `GET /v1/auth/me`, `web/app/login` Cloudflare-split (`w-[280px]` centered logo, no SSO placeholder, teal `#005a5e`), guard `localStorage aivory_mail_token` → `/login` | ✅ Live |
| **Admin Console** | `web/app/admin` (Overview/Users/Groups/Domains/Aliases/Logs) — research Zoho (users/aliases/groups, bulk import) + Gmail (30 aliases/user, delegation) + Mailflare (grid) | ✅ Live |
| **Users** | `GET/POST /v1/mailboxes`, `DELETE /v1/mailboxes/:id` (address/display_name/catch_all) + bulk via admin | ✅ |
| **Groups** | `GET/POST /v1/groups`, `DELETE /:id`, `POST /:id/members`, `DELETE /:id/members/:member_id` (`groups`, `group_members` tables) | ✅ |
| **Aliases (Send As)** | `GET/POST /v1/send-as?mailbox_id=` + `DELETE /:id` (domain must be `Active`, appears in Compose From dropdown) | ✅ |
| **Domains** | `GET/POST /v1/domains`, `DELETE /:id`, `GET /:id/dns`, `POST /:id/verify` (real `hickory-resolver` TXT), DKIM `aivory._domainkey` per domain, `web/app/domains` + `page.tsx` iframe `src="/domains"` | ✅ |
| **Audit** | `GET /v1/audit-logs`, `audit_logs` table (actor/target/mailbox/message) | ✅ |

Avatar dropdown (`web/app/page.tsx:459`): `showAvatar` → user `email`/`User ID` hash, `Available/Busy/Offline` (localStorage), `Admin Console → /admin`, `Quiet Mode`, `Subscription Free plan`, `SIGN OUT` (`doLogout`).

### 3.2 Core Mail

| Area | Detail | Status |
|------|--------|--------|
| Inbox | `GET /v1/messages?folder=Inbox&search=&page=&per_page=20` (≤100), `PUT /:id/read`, `POST /:id/move` (Inbox/Sent/Drafts/Spam/Trash/Archive), `POST /:id/star`, `POST /:id/share` (7-day JWT), `GET /:id/attachments/:att_id`, `GET /v1/threads` + `GET /:id` + `POST /:id/reply`, `GET /v1/threads/:id/crawl`, `WebSocket /v1/realtime/ws` | ✅ |
| Folders | System `Inbox/Sent/Drafts/Snoozed/Archive/Spam/Trash` + custom `GET/POST /v1/folders` (`folders` table, color `#006355`), `by_folder` counts via `GET /v1/stats` `GROUP BY folder` (Snoozed virtual `snoozed_until>now()`) | ✅ |
| Bulk | Check all (header checkbox), `selectedIds: Set<string>`, bulk bar `Read/Unread/Spam/Archive/Delete` + second row `Mark all as read/unread`, `Delete all` (parallel `PUT /read`/`DELETE`/`POST /move`, handles threads → expand messages), `refreshCounts` | ✅ |
| Compose | `web/components/ComposeModal.tsx` `From` (send-as dropdown), `To/Cc/Bcc`, `Subject`, `Body` (Text/HTML toggle), `Attachments` 10MB/20MB, `B/I/U` `wrapSelection("<b>")` via `bodyRef` (not emoji), `Send Later ▾` (1h/Tomorrow 9am/Monday/Pick → `scheduleAt` timer → `actuallySend`), `Undo 5-30s` (`pending` timer + `SendingBanner`), signature injection (`activeSig` multi-per-mailbox) | ✅ |
| Snoozed | `POST /v1/messages/:id/snooze` + `DELETE /unsnooze` (`snoozed_until`), virtual folder `Snoozed` (excluded from Inbox, ordered `snoozed_until ASC`) | ✅ |
| Labels | `GET/POST /v1/labels`, `DELETE /:id`, `message_labels` (`message_id/label_id`), `GET/POST /v1/messages/:id/labels`, `DELETE /:id/:label_id`, UI chips + `+ Label` select in detail (`web/app/page.tsx:640`) | ✅ |
| Search | `GET /v1/search?q=&folder=&limit=20` — LIKE + FTS hybrid, Cognee vector when `COGNEE_URL` (2s timeout), `GET /v1/inbox/overview` (total/unread/today/threads), `GET /v1/threads/:id/memory` (budgeted) | ✅ |
| Contacts | `GET /v1/contacts`, `POST /v1/contacts/block` (is_blocked → Spam) + `contacts` table | ✅ |
| Calendar | `GET/POST /v1/calendar/events` + `PUT/DELETE /:id`, `004` conferencing (`Meet/Teams/Zoom`), `web/app/calendar` week/month/day | ✅ |

### 3.3 User Settings — 10 Gmail/Zoho/Outlook parity (KV `user_settings` + tables, `GET/POST /v1/settings?category=X`)

| Category | Keys | Applied |
|----------|------|---------|
| General | `undo_send_seconds` (5/10/20/30), `density` (comfortable/compact/cozy → `rowPad`), `conversation_view` (true → threads for Inbox, false → messages), `page_size` (20/50/100) | ✅ |
| Inbox | `inbox_type`, `categories` | Stored (filter UI ready) |
| Compose | `default_font`, `font_size`, `always_show_cc/bcc`, `outbox_delay_minutes` | ✅ (`always_show` via `showCcBcc` default) |
| Appearance | `theme` (light/dark → `isDark` `bg-zinc-900`/`bg-[#f8f6ef]`, `reading_pane` right/bottom/no-split → `flex-col h-[380px]` or `fixed inset-0`) | ✅ |
| Notifications | `desktop_sound`, `new_mail_banner` (`Notification.requestPermission` + WS) | ✅ |
| Shortcuts | `enabled` + `c` compose, `e` archive, `r` reply, `/` search, `x` select, `s` star, `#` delete | ✅ |
| Storage | `days_to_sync`, `download_attachments_wifi_only` | Stored |
| Signatures | `002` multi-per-mailbox `is_default` | ✅ (compose + `web/app/settings/mail` CRUD) |
| Filters | `mail_filters` `criteria_json`/`action_json` → `aivory_mail_core::filters::resolve_folder` on inbound + `apply_filters` (first match wins) | ✅ (execution) |
| Labels | `mail_labels` | ✅ (attach UI) |
| Vacation | `vacation_responders` (`enabled`, `subject`, `body`, `interval_days`, `start_at/end_at`) + `vacation_deliveries` dedup → `maybe_send_vacation_reply` (auto-submitted check) | ✅ |
| Forwarding/POP/IMAP | `forward_to`, `keep_copy`, `pop_enabled`, `imap_enabled` + `forwarding_rules` | Forwarding **executed** (`maybe_forward` in `inbound.rs:72` → `send_email` via outbound, `keep_copy` handled), POP/IMAP stored |
| Send As | `send_as_aliases` | ✅ (From dropdown) |

All 10 tabs at `/settings/mail` + polling `appearance` every 3s for live theme.

### 3.4 AI / MCP

- `POST /v1/intelligence/analyze` heuristic (`intent`, `urgency`, `entities`, `suggested_actions`) + `AI_GATEWAY_URL/v1/ai/analyze-email` merge (8s), `POST /v1/intelligence/suggest` + `POST /v1/agent/actions` (`create_task` → `WORKFLOW_URL/webhook/agent-action`, `draft_reply` → `AI_GATEWAY_URL/v1/ai/draft-reply` fallback heuristic)
- `POST /mcp` streamable-http (`initialize/tools/list/tools/call`) `GET /v1/mcp/tools` tools: `search_mail`, `get_inbox_overview`, `get_thread_memory`, `get_knowledge_compile`, `send_mail` + `POST /v1/mcp/generate-link`
- Knowledge: `GET /v1/knowledge/compile?budget=...` (Cognee)
- Realtime: `RealtimeHub` (`/v1/realtime/ws?mailbox_id=`)

---

## 4. API

Envelope `{success:boolean, data|error}`. See `docs/API.md` + `docs/openapi.json` (regen `python3 scripts/gen_openapi.py`).

| Method | Path | Note |
|--------|------|------|
| `POST` | `/v1/auth/login` | `{email,password}` → `{token,email,expires_at}` |
| `GET` | `/v1/auth/me` | health |
| `GET/POST` | `/v1/domains`, `GET/DELETE /:id`, `POST /:id/verify`, `GET /:id/dns`, `GET /:id/dkim` |  |
| `GET/POST` | `/v1/mailboxes`, `GET/PUT/DELETE /:id` |  |
| `GET` | `/v1/messages?mailbox_id=&folder=&search=&page=&per_page=` | Snoozed virtual |
| `GET/DELETE` | `/v1/messages/:id`, `PUT /:id/read`, `POST /:id/move`, `POST /:id/star`, `POST /:id/share`, `GET /:id/attachments/:att_id`, `POST /:id/snooze` |  |
| `GET/POST` | `/v1/messages/:id/labels`, `DELETE /:id/labels/:label_id` |  |
| `GET` | `/v1/threads`, `GET /:id`, `POST /:id/reply`, `GET /:id/crawl`, `GET/POST /:id/follow-up`, `GET /:id/memory` |  |
| `POST` | `/v1/send`, `POST /v1/send/batch` | DKIM signed |
| `POST` | `/v1/intelligence/analyze`, `POST /v1/intelligence/suggest`, `POST /v1/agent/actions` |  |
| `POST` | `/v1/webhooks/inbound`, `POST /v1/webhooks/cloudflare` |  |
| `GET` | `/v1/search?q=&limit=`, `GET /v1/inbox/overview`, `GET /v1/threads/:id/memory` |  |
| `GET/POST` | `/v1/signatures`, `PUT/DELETE /:id` |  |
| `GET/POST` | `/v1/drafts`, `GET /v1/cognee/sync`, `GET /v1/mcp/tools`, `POST /mcp` |  |
| `GET/POST` | `/v1/settings?category=X`, `GET/POST /v1/labels`, `DELETE /:id`, `GET/POST /v1/filters`, `GET/POST /v1/vacation`, `GET/POST /v1/folders`, `DELETE /:id` |  |
| `GET/POST` | `/v1/groups`, `DELETE /:id`, `POST /:id/members`, `DELETE /:id/members/:member_id` |  |
| `GET/POST` | `/v1/contacts`, `POST /block`, `GET /v1/send-as`, `POST /`, `DELETE /:id` |  |
| `GET` | `/v1/stats`, `GET /v1/calendar/events` etc, `GET /health`, `GET /v1/realtime/ws` |  |

---

## 5. Development

```bash
# API (SQLite, no setup)
cp .env.example .env # DATABASE_URL=sqlite://./data/mail.db, JWT_SECRET, INTERNAL_TOKEN
cargo run --bin aivory-mail-api # :8095/health
# Web
cd web && npm install --legacy-peer-deps && npm run dev # :3005
# SMTP ingress (VPS)
cargo run --bin aivory-mail-smtp # :2525
```

Env: `PORT=8095`, `DATABASE_URL`, `STORAGE_BACKEND=local|r2|s3`, `JWT_SECRET`, `INTERNAL_TOKEN`, `MAIL_MODE=vps|cloudflare|hybrid`, `CF_API_TOKEN`, `CF_ZONE_ID`, `SMTP_HOST/PORT/USER/PASSWORD`, `AI_GATEWAY_URL`, `WORKFLOW_URL`, `COGNEE_URL`, `CORS_ORIGINS`, `MAIL_MX_HOST` (default `mail.aivory.uk` prod), `MAIL_ADMIN_EMAIL/PASSWORD` (default `admin@aivory.id/Avry786876!@`, superadmin `irfan.reichmann@aivory.uk` via `SUPERADMIN_EMAIL`), `INSPECTION_MODE=true` (allow any email + correct password for VPS demo).

Migrations: `001_initial.sql` … `011_audit_logs`, `ensure_schema` idempotent for SQLite (`main.rs:71`). Regenerate OpenAPI: `python3 scripts/gen_openapi.py`.

Web: `NEXT_PUBLIC_MAIL_API` (default `http://localhost:8095`), `NEXT_PUBLIC_BOOK_URL`, `NEXT_PUBLIC_MAIL_MX_HOST`, `Manrope` font, `npm install --legacy-peer-deps` (next@15 vs react@19), Tailwind v4.

---

## 6. Deployment

### Docker (VPS)

```bash
docker network create aivory-network || true
docker compose -f docker-compose.traefik.yml up -d
docker compose -f docker-compose.production.yml up -d --build
# avry-mail :8095 (API) + avry-mail-web :3005 (Next.js) via Traefik Host(`mail.aivory.uk`)
curl https://mail.aivory.uk/health
```

`services/avry-mail/web/Dockerfile` multi-stage Node 20 (`builder` `npm ci` + `npm run build` with `ARG NEXT_PUBLIC_*` → `runner` non-root, healthcheck `wget :3005`). `docker-compose.production.yml` adds `avry-mail-web` (priority 10 for `/`, API handles `/v1/` + `/health`).

### Cloudflare Worker

`worker/worker.js` `email()` → `POST $AIVORY_MAIL_API_URL/v1/webhooks/cloudflare`, `wrangler deploy`. Enable Email Routing zone `Email → Enable → Add route hello@example.com → Worker aivory-mail-worker`, point `NEXT_PUBLIC_MAIL_API`.

### DNS

```
MX  @  mail.aivory.uk (or route.mx.cloudflare.net if CF)
TXT @  v=spf1 include:_spf.mx.cloudflare.net ~all
TXT _dmarc  v=DMARC1; p=quarantine;
TXT aivory._domainkey  v=DKIM1; k=rsa; p=<dkim_public_key>
```

For Tencent (port 25 blocked): use `MAIL_MODE=cloudflare` + `CF_API_TOKEN=cfut_...` (valid `eb26a4d6...`, `cfk_...` invalid) + `CF_ZONE_ID=518089ea...` for `aivory.uk` (Cloudflare Email Service relay, no VPS SMTP). Keep `mail.aivory.uk A 129.226.155.216` **Proxied** for WEB (orange), create `mx.aivory.uk A 129.226.155.216 DNS only` for MX if need VPS inbound.

---

## 7. Login & Admin

- `web/app/login` Cloudflare-split (left form `w-[380px]` centered logo `w-[280px]` `aivory-mail-logo2.svg`, right `Aivory Connect 2026` gradient `from-[#005a5e]` dot map) — no emoticon, outline `Ico` lock/eye `P.lock/P.eye`.
- Guard `page.tsx:118` `localStorage aivory_mail_token` → `/login` if missing, `doLogout` clear.
- Avatar top-right (`showAvatar` dropdown): `irfan.reichmann@aivory.uk` (from token), `User ID` hash, `My Account → /settings/mail`, `Available/Busy/Offline` (localStorage), `Admin Console → /admin`, `Quiet Mode`, `Subscription Free plan → Upgrade → /admin`, `SIGN OUT`.
- `web/app/admin` tabs `overview/users/groups/domains/aliases/logs` — create/delete mailboxes/groups/aliases, `POST /v1/groups`.

Demo: `admin@aivory.id / Avry786876!@` or any mailbox + same password (inspection), or `irfan.reichmann@aivory.uk / Avry786876!@` (superadmin).

---

## 8. VPS Check (Tencent `129.226.155.216:63222` `ubuntu` via `tencent-vps` alias `~/.ssh/claude_code_vps`)

```bash
ssh tencent-vps "docker ps | grep avry-mail; curl -f http://localhost:8095/health; curl -f http://localhost:3005/ | head"
# before: avry-mail not running, AVRY-V2-Main at 0812137 (old)
# after pull: git -C ~/avry-v2-main-src pull origin main (or AVRY-Mail directly: git clone https://github.com/Aivory-hub88/AVRY-Mail && docker compose up -d --build)
```

---

*Last verified: `cargo check` 15 warnings, `cargo test` 3 passed, `npm run build` 8 routes (15.4kB /), `curl :8095/health` ok, `curl :3005/login` Cloudflare-split teal. If claim contradicts `USER_SETTINGS.md:Status` or `README.md:Roadmap`, those tables win — file a fix.*
