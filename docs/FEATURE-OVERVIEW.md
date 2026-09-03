# AVRY-Mail — Product Overview

> One document to explain what Aivory Mail *is* as a product, what it actually does today, and where it's going. Detail-level references live in the five docs already beside this one — this is the entry point.

---

## 1. What Aivory Mail is

**Aivory Mail is business email infrastructure you own.** No per-message SaaS markup, no vendor lock on your mail data. Deploy it on a VPS, on Cloudflare, or both — same Rust core, same Postgres/SQLite-compatible migrations, same Next.js web.

### Mailflare lineage — and where it diverges

Aivory Mail's product shape was inspired by [Mailflare](https://github.com/hieunc229/mailflare) — a self-hosted Cloudflare-native inbox (2.2k stars, Cloudflare Email Routing → D1 + R2). Mailflare proved the model: connect a custom domain, let Cloudflare handle ingress, keep mail in your own D1/R2.

Aivory Mail **reimplements that idea in Rust, with no direct code copy**. Mailflare is Next.js + TypeScript on Cloudflare Workers; its license forbids a commercial SaaS derivative. The reimplementation is deliberate and already credited in `README.md:Credits`:

> *Inspired by Mailflare (Cloudflare-native) and mailcow-dockerized (production hardening). Core reimplemented in Rust; no direct code copy (Mailflare license forbids commercial SaaS derivative).*

What Aivory adds that Mailflare itself doesn't have:

| Capability | Mailflare | Aivory Mail |
|------------|-----------|-------------|
| Inbound transport | Cloudflare Email Routing only | **SMTP ingress `:2525` (Rust `aivory-mail-smtp`) +** Cloudflare Email Routing + hybrid fallback |
| Outbound signing | Cloudflare Email Service | **Rust-native MTA path**: `lettre` relay, DKIM selector stored (`aivory._domainkey`, `001_initial.sql:16`, `domains.rs:41`), SPF/DMARC guidance, STARTTLS advertisement (`aivory-mail-smtp/src/main.rs:58`) — DKIM *signing* is the next step, not yet signing every message |
| Storage | D1 + R2 only | `DbPool` Postgres/SQLite + `ObjectStore` local/R2/S3 (swappable) |
| AI / Workflow | none | Heuristic intelligence + AI gateway merge, MCP server, n8n/Cerveau hooks |
| Tenancy | single Cloudflare account | Tenant-aware (`tenant_id` default `default`) across API |

The core is **Cloudflare-compatible, not Cloudflare-dependent** (`MAIL_MODE=vps|cloudflare|hybrid`, `ARCHITECTURE.md:Multi-environment`). Everything internal works identically on a plain VPS.

### Who it's for

Teams that want a custom-domain inbox they control (agency domains, product domains, client workspaces), with the option to keep Cloudflare Email Routing's zero-ops inbound *or* run a fully self-hosted SMTP path on the same binary.

---

## 2. Feature matrix — honest status, not aspirational

Single table. "Done" means persisted, exposed via API, and reachable from the web UI. "Partial" / "Stored but not yet wired" means schema + API exist but the behavior isn't applied in the mail pipeline yet. This mirrors `USER_SETTINGS.md:Status` exactly — not re-derived.

### 2.1 Core mail

| Area | What's real today | Status |
|------|-------------------|--------|
| **Custom domains** | Create/list/delete, `validate_domain` + `normalize`, `status Pending→Active` (`domains.rs:32-59`), DKIM selector `aivory` persisted per domain, `GET /v1/domains/:id/dns` live — CF mode fetches real DNS via `CfClient::get_dns_records`, VPS mode returns manual MX/SPF/DKIM checklist. Live checklist + verification flow already merged. | ✅ Done |
| **Mailboxes** | `POST /v1/mailboxes` with `address/display_name/is_catch_all/forward_to`, tenant-scoped, CF mode auto-enables Email Routing (`domains.rs:52-56`). | ✅ Done |
| **Inbound mail** | VPS: SMTP ingress `:2525` → `POST /v1/webhooks/inbound` (raw MIME via `mail-parser`); Cloudflare: `worker/worker.js` → `POST /v1/webhooks/cloudflare`; same persistence pipeline (message + thread + realtime fan-out). Real MTA behavior, not a mock. | ✅ Done |
| **Outbound mail** | `POST /v1/send` + `POST /v1/send/batch` (≤50), stored + queued via `lettre` in VPS mode, Cloudflare Email Service in CF mode, `hybrid` tries CF then fallback SMTP. Attachments, cc/bcc, STARTTLS advertised. DKIM *signing* is roadmap (below). | ✅ Done (signing = next) |
| **Inbox / threads** | `GET /v1/messages?folder=Inbox&search=&page=&per_page=` (≤100), `GET /v1/threads`, `PUT /v1/messages/:id/read`, `POST /v1/messages/:id/move` (Inbox/Sent/Drafts/Spam/Trash/Archive), star, share link (7-day JWT), attachment download, thread crawl/memory, WebSocket realtime (`/v1/realtime/ws`). | ✅ Done |
| **Compose** | Web `app/page.tsx` inbox compose + `POST /v1/threads/:id/reply`, signature injection (`activeSig` multi-per-mailbox, `002` migration), drafts (`/v1/drafts`). | ✅ Done (multi-signature already shipped) |
| **Calendar** | Google-parity week/month/day, `GET/POST /v1/calendar/events` + `PUT/DELETE /:id`, conferencing prefs (Meet/Teams/Zoom, `004`), proposals, booking/slots. Wired at `web/app/calendar`. Per-mailbox isolation added `009` — see `CALENDAR.md`. | ✅ Done |

### 2.2 User settings — 10 Gmail/Zoho/Outlook parity categories

Backed by `user_settings` KV table (`006_settings.sql`) at `GET/POST /v1/settings?category=X`. The API seeds defaults (`settings.rs::default_for`) so every `GET` returns a complete object even before any `POST`.

> This section **reuses the exact status table from `USER_SETTINGS.md:Status`** — don't re-derive it. Any future wiring change updates that table; this doc mirrors it.

| Feature | Schema | API | UI |
|---------|--------|-----|-----|
| General (undo/density/conversation/page) | ✅ | ✅ | ✅ |
| Inbox type + categories | ✅ | ✅ | ✅ |
| Compose (font/cc/bcc/delay) | ✅ | ✅ | ✅ |
| Appearance (theme/pane) | ✅ | ✅ | ✅ |
| Notifications | ✅ | ✅ | ✅ |
| Shortcuts | ✅ | ✅ | ✅ |
| Storage & Offline | ✅ | ✅ | ✅ |
| Signatures | ✅ (002) | ✅ | ✅ (inbox modal) |
| Filters | ✅ | ✅ | ✅ |
| Labels | ✅ | ✅ | ✅ |
| Vacation responder | ✅ | ✅ | ⏳ (state ready, UI partial) |
| Forwarding / POP / IMAP | ✅ | ✅ (KV) | ✅ (KV) |
| Send-as aliases | ✅ | ⏳ | ⏳ |
| Forwarding rules table | ✅ | ⏳ | ⏳ (uses KV) |

**What "actually applied" vs "stored but not yet wired" means in practice:**

- **Actually applied today:** General/Inbox/Compose/Appearance/Notifications/Shortcuts/Storage prefs read at boot and honored by the web UI (density, conversation view, page size 20/50/100, font, theme light/dark, reading pane right/bottom/no-split, shortcuts Gmail-style, etc.). Labels and filters are CRUD-complete and persisted; filter *execution* against inbound mail (Sieve-style) is still the roadmap item below. Signatures: multi-per-mailbox with `is_default` already drives compose injection.
- **Stored but not yet wired:** Vacation responder state is per-mailbox (`vacation_responders` with `interval_days`, `start_at/end_at`) and the `POST /v1/vacation` contract is stable, but the auto-reply path isn't yet triggered on every inbound delivery (UI is partial). Send-as aliases and `forwarding_rules` tables are migrated (`006`) and ready — the send path still uses the mailbox's own `address` / KV `forward_to` (no alias selector in compose, no auto-forward pipeline). POP/IMAP toggles are KV-backed prefs. These are the next wiring sprints in the 10-feature MVP cut (General undo+density+conversation + Filters + Vacation + Send As — see `USER_SETTINGS.md:Parity reference`).

The 10 tabs live at `/settings/mail`: General · Inbox · Signatures · Compose · Filters & Labels · Forwarding & POP/IMAP · Appearance · Notifications · Shortcuts · Storage & Offline.

### 2.3 Calendar, AI, MCP

| Area | What's real | Status |
|------|-------------|--------|
| **Calendar** | See 2.1 — full CRUD, conferencing, proposals | ✅ Done |
| **Intelligence** | `POST /v1/intelligence/analyze` — offline heuristic (intent/urgency/entities/suggested_actions) + optional merge with `AI_GATEWAY_URL/v1/ai/analyze-email`; async triggers `WORKFLOW_URL/webhook/email-received` + `AI_GATEWAY_URL/v1/mail/intelligence`; knowledge compile `GET /v1/knowledge/compile` (cognee). Example in `README.md:API`. | ✅ Done |
| **MCP** | Remote streamable-http at `POST /mcp` (JSON-RPC `initialize/tools/list/tools/call`), `GET /v1/mcp/tools`, link generation `POST /v1/mcp/generate-link`; tools: `search_mail`, `get_inbox_overview`, `get_thread_memory`, `get_knowledge_compile`, `send_mail`. Used by Cerveau / `avry-zeroclaw` sidecar. | ✅ Done |
| **API keys** | Tavily-style `/v1/api-keys` — SHA-256 hashed, `key_raw avry-…` kept for consistent masked+reveal (fixed `2e0a719`). | ✅ Done |

### 2.4 Known stubs (explicit, not oversold)

- DKIM *signing* and SPF/DMARC *verification* on every outbound/inbound — selector + DNS guidance shipped, signing is roadmap.
- Sieve filtering (filters are stored; not yet executed on ingress).
- Full MIME `multipart/mixed` inline rendering for complex attachments.
- S3/R2 presigned URLs + streaming downloads (local `ObjectStore` today).
- D1 HTTP binding for Cloudflare Workers (hybrid works via Postgres).
- Admin domain onboarding wizard + audit log + retention policies.

These live in `README.md:Roadmap` — not hidden, not promised as shipped.

---

## 3. Architecture at a glance

Condensed from `ARCHITECTURE.md`. Full detail stays there.

```
                Aivory Identity (JWT / tenant)
                        │
                ┌───────▼────────┐
                │  Aivory Mail   │ ← this repo
                └───────┬────────┘
         ┌──────────────┼──────────────┬──────────────┐
         │              │              │              │
    Cloudflare      Aivory DB     Aivory Files    Aivory Web
    Email Layer   (Postgres/                  (Next.js :3005)
                   SQLite)        (R2/S3/local)
         │              │              │              │
         └──────────────┼──────────────┴──────────────┘
                        │
               ┌────────▼────────┐
               │   AI Gateway    │ ← Cerveau / ZeroClaw / OpenRouter
               └────────┬────────┘
                        │
               ┌────────▼────────┐
               │ Aivory Workflow │ ← n8n-as-code / n8n :3500
               └─────────────────┘
                        │
               CRM · Tasks · Office
```

**Multi-env** (`MAIL_MODE` — `DEVELOPMENT.md:Env reference`, `ARCHITECTURE.md:Multi-environment`):

| Mode | Inbound | Outbound | Storage | DB |
|------|---------|----------|---------|-----|
| `vps` | SMTP ingress `:2525` → API | SMTP relay (`lettre`) | local / S3 | Postgres / SQLite |
| `cloudflare` | Email Routing → Worker `email()` → `/v1/webhooks/cloudflare` | Cloudflare Email Service | R2 | D1 / Postgres |
| `hybrid` | both | CF first, fallback SMTP | R2 or S3 | Postgres |

**Crate / layout** (from `ARCHITECTURE.md:Workspace layout`):

```
crates/
  aivory-mail-core/     # types, MIME parsing (mail-parser), routing, intelligence heuristics
  aivory-mail-storage/  # DbPool (Postgres/SQLite) + ObjectStore (local/R2/S3)
  aivory-mail-api/      # Axum HTTP API + realtime hub + mail handlers (main.rs, api/*, mail/*)
  aivory-mail-smtp/     # VPS SMTP ingress (minimal SMTP → webhook forwarder)  :2525/:2587
web/  app/              # Next.js App Router — / (inbox), /calendar, /settings, /settings/mail, /share/[id]
worker/worker.js        # Cloudflare Worker shim (Email Routing → API)
migrations/             # 001_initial → 006_settings (Postgres + SQLite compatible)
```

API is `Axum` (`aivory-mail-api/src/main.rs`) with `DbPool` enum (every query has Postgres `$1` + SQLite `?` variants), `RealtimeHub` at `/v1/realtime/ws`, permissive CORS (`CORS_ORIGINS`). Web calls `NEXT_PUBLIC_MAIL_API` (default `http://localhost:8095`).

---

## 4. Roadmap

Pulled from `README.md:Roadmap` (canonical). Checkmarks below are *not-yet* unless noted.

- [ ] **DKIM signing** (`lettre` dkim) + SPF/DMARC verification — selector persisted today, signing next
- [ ] **Sieve filtering** — run `mail_filters` on ingress (criteria_json → action_json)
- [ ] **Full MIME attachment inline** (`multipart/mixed` rendering)
- [ ] **S3/R2 presigned URLs + streaming downloads** — `ObjectStore` trait ready, local today
- [ ] **D1 HTTP binding** for Cloudflare Workers (full Worker-native deploy without Postgres)
- [ ] **Admin UI: domain onboarding wizard** (DNS live-check — `GET /v1/domains/:id/dns` already returns CF records; wizard is the UI pass)
- [ ] **Audit log + retention policies**

### UI direction — Mailflare-inspired redesign (in progress / next)

Mid-way through scoping, the user corrected the visual direction: **follow Mailflare's actual look** — light, blue-accented, spacious — combined with the motion/UX already shipped in Aivory (Emil-style transitions, `dialog-overlay-show`/`dialog-content-show` scale micro-animations, `prefers-reduced-motion` guard).

Mailflare's concrete tokens (audited from `tmp/mailflare/src`):

- **Palette:** `--background #f6f8fc`, `--card #ffffff`, `--muted #eef3fb`, `--border #dadce0`, `--primary #0b57d0` (`mailflare/src/app/globals.css`)
- **Typography:** Geist Sans/Mono (`layout.tsx`), not Manrope — Aivory currently uses Manrope (`web/app/layout.tsx`); the combine keeps Manrope as brand but adopts Mailflare's spacing/rhythm
- **Layout:** `grid [var(--sidebar-width) 1fr] h-dvh`, sidebar `px-3 py-4`, header `h-16`, main `rounded-tl-3xl bg-white` (`mailflare/src/app/(dashboard)/layout.tsx`) — spacious rounded-card with badge pills and subtle hover `#f2f6fc` + `shadow-sm` on message rows (`message-folder-page.tsx`)
- **Nav:** Compose primary action + folder links with live counts + custom folders with drag-drop and color dots (`dashboard-nav.tsx`)

Aivory's *current* inbox (`web/app/page.tsx`) is darker: `bg-zinc-50`, `border-zinc-200`, `bg-zinc-900` primary, active folder `bg-zinc-900 text-white`. The redesign merges the two: **Mailflare's light blue-accented card language + Aivory's existing inbox/compose/calendar/sharing surfaces + the motion work already built** (dialog scale, grid transition `duration-200`, message navigation progress). **No behavior changes** in this pass — this doc lands first, redesign is a separate follow-up.

For the MVP feature cut that follows the redesign, the ROI order already agreed is: **General (undo 5–30s + density + conversation view) + Filters + Vacation responder + Send As alias** — the 10-feature wiring listed in §2.2.

---

## 5. Where to read next

This doc is the narrative entry point. Detail lives in the five reference docs — linked, not duplicated:

| Doc | What it covers | When to read it |
|-----|----------------|-----------------|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | System map, `MAIL_MODE`, crate layout, request lifecycles (inbound VPS vs Cloudflare, outbound), DB migrations 001→006, realtime/AI/MCP specifics | Before touching backend or storage |
| [`DEVELOPMENT.md`](DEVELOPMENT.md) | Ports (`8095/3005/2525/2587/5436`), SQLite vs Postgres quickstarts, SMTP ingress run, `NEXT_PUBLIC_MAIL_API`, `.env` table, gotchas (`--legacy-peer-deps`, `key_raw` shard) | Local dev setup |
| [`API.md`](API.md) | Full endpoint table + envelope `{success,data|error}`, every route from `/health` to `/mcp` | API integration, MCP client, share links |
| [`USER_SETTINGS.md`](USER_SETTINGS.md) | 10 user-settings categories: storage model (`user_settings` KV + `mail_filters/labels/vacation_responders/send_as_aliases/forwarding_rules`), keys & defaults, `/settings/mail` 10-tab UI, Gmail/Zoho/Outlook parity research, **canonical status table** (reuse, don't re-derive) | Settings wiring, next-MVP cut |
| [`CALENDAR.md`](CALENDAR.md) | Calendar schema, per-mailbox isolation model (and its limits), API surface, relationship to Calnode (`book.aivory.uk`) | Touching calendar code, or explaining "Aivory Cal" |
| [`DEPLOYMENT.md`](DEPLOYMENT.md) | `docker-compose.yml` (API/DB/SMTP + Traefik labels for `mail.aivory.uk:8095`), Worker deploy (`worker.js` + `wrangler.jsonc`), DNS (MX/SPF/DMARC/DKIM), prod `.env`, submodule workflow (`services/avry-mail` in `Aivory V2`) | VPS or Cloudflare deploy |
| [`openapi.json`](openapi.json) | Generated OpenAPI spec | Codegen / SDK |

Other Aivory-wide context: `docs/AGENT-FEATURE-OVERVIEW.md` in the `Aivory V2` entry repo (separate narrative doc for agents — same convention this doc follows).

---

*Last verified against `USER_SETTINGS.md:Status` and `README.md:Roadmap` at doc creation. If a claim here contradicts those tables, those tables win — file a fix.*
