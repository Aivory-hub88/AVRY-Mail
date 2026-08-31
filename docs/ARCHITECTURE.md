# Aivory Mail — Architecture

> Business email infrastructure, without the per-message SaaS markup.
> Rust core, Next.js UI, Cloudflare + VPS compatible, Cerveau/Workflow connected.

## System map

```
                    ┌─────────────────────┐
                    │   Aivory Identity    │   auth / JWT / tenant
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │     Aivory Mail     │ ← this repo
                    └──────────┬──────────┘
                    ┌──────────┼──────────┬──────────────┐
                    │          │          │              │
            Cloudflare    Aivory DB    Aivory Files    Aivory Web
            Email Layer   (Postgres/   (R2/S3/local)   (Next.js :3005)
                          SQLite)
                    │          │          │              │
                    └──────────┼──────────┴──────────────┘
                               │
                    ┌──────────▼──────────┐
                    │     AI Gateway      │ ← Cerveau / ZeroClaw / OpenRouter
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │  Aivory Workflow    │ ← n8n-as-code / avry-n8n :3500
                    └─────────────────────┘
```

## Multi-environment (`MAIL_MODE`)

| Mode         | Inbound                              | Outbound                             | Storage      | DB               |
|--------------|--------------------------------------|--------------------------------------|--------------|------------------|
| `vps`        | SMTP ingress `:2525` → API           | SMTP relay (lettre)                  | local / S3   | Postgres / SQLite |
| `cloudflare` | Email Routing → Worker → `/v1/webhooks/cloudflare` | Cloudflare Email Service | R2        | D1 / Postgres     |
| `hybrid`     | both                                 | CF first, fallback SMTP              | R2 or S3     | Postgres         |

The core is **Cloudflare-compatible, not Cloudflare-dependent**. Everything
internal works identically on VPS with plain Postgres + SMTP.

## Workspace layout

```
crates/
  aivory-mail-core/     # types, MIME parsing (mail-parser), routing, intelligence heuristics
  aivory-mail-storage/  # DbPool (Postgres/SQLite) + ObjectStore (local/R2/S3)
  aivory-mail-api/      # Axum HTTP API, realtime hub, mail handlers
  aivory-mail-smtp/     # VPS SMTP ingress (minimal SMTP → API forwarder)
web/
  app/                  # Next.js UI — inbox, calendar, settings
worker/
  worker.js             # Cloudflare Worker shim (Email Routing → API)
migrations/             # sqlx migrations (Postgres + SQLite compatible)
docs/
  openapi.json          # generated API spec
scripts/
  deploy-vps.sh         # VPS deployment helper
```
## Backend (axum)

Entry point: `crates/aivory-mail-api/src/main.rs`.

- Config from env (`Config::from_env`), defaults for local dev (see `config.rs`).
- DB: `DbPool` enum over `Postgres` / `Sqlite` — every query is written twice
  (once per backend) to keep both fully supported.
- Storage: `ObjectStore` trait with `LocalStore` (and hooks for R2/S3). Used for
  raw message blobs + attachments.
- Realtime: `RealtimeHub` + WebSocket at `/v1/realtime/ws`.
- CORS: permissive by default (`*`), tighten via `CORS_ORIGINS`.

### Request lifecycle (outbound)

```
POST /v1/send
  → SendRequest (from/to/subject/body)
  → outbound::require_verified_sender_domain — reject unless the `from`
    domain is Active with a DKIM key on file
  → build MIME message → sign with the domain's DKIM key (mail-auth)
  → store raw message + attachment blobs
  → queue signed-raw SMTP delivery (lettre send_raw, vps mode) or Cloudflare
  → respond { id, status: "queued" }
```

### Request lifecycle (inbound, VPS)

```
SMTP ingress (:2525) → RCPT TO
  → GET /v1/internal/resolve-recipient (mail::routing) — 550 5.1.1 if no
    mailbox/catch-all matches, before DATA is even accepted
  → DATA → /v1/webhooks/inbound
  → parse MIME (aivory-mail-core::parser)
  → resolve_recipient again (webhook-path backstop) → reject if unmatched
  → persist message + thread
  → optional: AI intelligence/webhook → workflows
  → realtime WS fan-out
```

### Custom domains

`domains::create` generates a per-domain verification token and RSA-2048
DKIM keypair immediately. `GET /v1/domains/:id/dns` computes the full
MX/SPF/DKIM/DMARC/verification checklist (`aivory_mail_core::dns`) and
checks it against live public DNS (`mail::dns_check`, hickory-resolver) —
this works for any domain regardless of DNS host, not just Cloudflare
zones. `POST /v1/domains/:id/verify` does a real TXT lookup before flipping
`Pending` → `Active`; nothing is marked verified without it.

### Request lifecycle (inbound, Cloudflare)

```
Cloudflare Email Routing → worker.js (email handler)
  → POST AIVORY_MAIL_API_URL/v1/webhooks/cloudflare
  → same persistence pipeline
```

## Frontend (Next.js)

`web/` — App Router, Tailwind v4, Manrope (next/font).

| Route              | Purpose                                    |
|--------------------|--------------------------------------------|
| `/`                | Inbox: list, thread view, compose, star, share, signature modal |
| `/calendar`        | Google-parity week/month/day calendar + events CRUD |
| `/settings`        | API keys (Tavily-style, masked + reveal) + Remote MCP |
| `/settings/mail`   | User settings — 10 Gmail/Zoho/Outlook parity tabs |
| `/share/[id]`      | Public read-only shared message            |

## Database

Six migrations, all **Postgres + SQLite compatible**:

| File                         | Content                                              |
|------------------------------|------------------------------------------------------|
| `001_initial.sql`            | tenants, domains, mailboxes, threads, messages, attachments, api_keys |
| `002_signatures_calendar.sql`| signatures (multi-per-mailbox), calendar_proposals   |
| `003_calendar_events.sql`    | calendar_events (Google parity CRUD)                 |
| `004_conferencing.sql`       | + conferencing, conferencing_link on events          |
| `005_knowledge_cache.sql`    | knowledge_cache (agent compile cache)                |
| `006_settings.sql`           | user_settings, mail_filters, mail_labels, vacation_responders, send_as_aliases, forwarding_rules |

`main.rs` also runs an `ensure_schema()` idempotent bootstrap for sqlite, so a
missing migration never blocks first boot.

## AI / Intelligence

- `POST /v1/intelligence/analyze` — heuristic (offline) + optional AI gateway merge.
- `POST /v1/agent/actions` — dispatch to agent runtime.
- Remote **MCP server** at `/mcp` (streamable-http). Tools exposed:
  `search_mail`, `get_inbox_overview`, `get_thread_memory`,
  `get_knowledge_compile`, `send_mail`.
- Knowledge compiler: `GET /v1/knowledge/compile` caches per-tenant/scope
  compiled context for agents (`knowledge_cache`).

## Related repos (Aivory V2)

This repository is registered as a **git submodule** in `Aivory V2`:

```bash
cd ~/Documents/"Aivory V2"
git submodule update --init services/avry-mail   # → Aivory-hub88/AVRY-Mail
```

Sibling services, all submodules too: `avry-backend`, `avry-user-dashboard`,
`avry-admin-dashboard`, `avry-console`, `avry-n8n`, `avry-zeroclaw`,
`avry-payments`, `cerveau` (cognee), etc.

## Conventions

- All API responses use `{ success: bool }` envelope, `data` for payloads,
  `error` for failures.
- SQL is written in **both** `$1` (Postgres) and `?` (SQLite) variants in the
  same handler.
- IDs: `UUID` everywhere (Postgres typed `UUID`, SQLite `TEXT`).
- Timestamps: ISO-8601 UTC strings (`chrono` rfc3339).