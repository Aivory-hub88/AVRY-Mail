# Aivory Mail — Business email, without the email tax.

> Rust mail core + multi-environment (Cloudflare / VPS) + AI triage + Workflow triggers. Cerveau-compatible.

![Rust](https://img.shields.io/badge/Rust-1.82-orange) ![License](https://img.shields.io/badge/license-Proprietary-red)

**Repo:** https://github.com/Aivory-hub88/AVRY-Mail

---

## Architecture

```
                ┌──────────────────┐
                │  Aivory Identity │
                └────────┬─────────┘
                         │
                ┌────────▼─────────┐
                │   Aivory Mail    │  ← this repo
                └────────┬─────────┘
                         │
          ┌──────────────┼──────────────┐
          │              │              │
     Cloudflare      Aivory DB      Aivory Files
     Email Layer      (Postgres)     (R2/S3/local)
          │              │              │
          └──────────────┼──────────────┘
                         │
                ┌────────▼─────────┐
                │   AI Gateway     │  ← Cerveau / ZeroClaw / OpenRouter
                └────────┬─────────┘
                         │
                ┌────────▼─────────┐
                │  Aivory Workflow │  ← n8n / Aivory Workflow
                └────────┬─────────┘
                         │
       ┌─────────────────┼──────────────────┐
       ↓                 ↓                  ↓
      CRM              Tasks             Office
```

### Multi-env

| Mode | Inbound | Outbound | Storage | DB |
|------|---------|----------|---------|------|
| `vps` | SMTP ingress `:2525` | SMTP relay / lettre | local or S3 | Postgres / SQLite |
| `cloudflare` | Email Routing → Worker `email()` → `/v1/webhooks/cloudflare` | Cloudflare Email Service API | R2 | D1 / Postgres |
| `hybrid` | both | try Cloudflare then fallback SMTP | R2 or S3 | Postgres |

`MAIL_MODE` env controls it. The core is **Cloudflare-compatible, not Cloudflare-dependent**.

---

## Crates

```
crates/
  aivory-mail-core/      # types, MIME parser (mail-parser), routing, intelligence heuristics
  aivory-mail-storage/   # DbPool (Postgres/SQLite) + ObjectStore (local / R2/S3)
  aivory-mail-api/       # Axum HTTP API + realtime hub + mail handlers
  aivory-mail-smtp/      # VPS SMTP ingress (minimal SMTP → webhook forwarder)
worker/
  worker.js              # Cloudflare Worker shim (Email Routing → API)
web/
  app/                   # Next.js inbox UI
migrations/
  001_initial.sql
```

---

## Quick start (local)

```bash
# Rust
cargo run --bin aivory-mail-api
# → http://localhost:8095/health

# With Postgres (docker)
docker compose up -d --build
curl http://localhost:8095/health
```

### Env

Copy `.env.example` → `.env` and fill `DATABASE_URL`, `JWT_SECRET`, `INTERNAL_TOKEN`.

For dev without Postgres:

```bash
DATABASE_URL=sqlite://./data/mail.db
STORAGE_BACKEND=local
MAIL_MODE=vps
```

---

## API

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health |
| GET/POST | `/v1/domains` | Domains |
| GET/DELETE | `/v1/domains/:id` | Domain detail |
| POST | `/v1/domains/:id/verify` | Mark verified |
| GET | `/v1/domains/:id/dns` | DNS status (CF) |
| GET/POST | `/v1/mailboxes` | Mailboxes |
| GET | `/v1/messages?mailbox_id=&folder=Inbox&search=&page=` | List messages |
| GET | `/v1/messages/:id` | Get message (marks read) |
| PUT | `/v1/messages/:id/read` | Toggle read |
| POST | `/v1/messages/:id/move` | Move folder |
| GET | `/v1/messages/:id/attachments/:att_id` | Download attachment (R2) |
| GET | `/v1/threads` | Threads |
| GET | `/v1/threads/:id` | Thread + messages |
| POST | `/v1/threads/:id/reply` | Reply |
| POST | `/v1/send` | Send email |
| POST | `/v1/send/batch` | Batch send |
| POST | `/v1/intelligence/analyze` | AI analyze |
| POST | `/v1/agent/actions` | Agent action dispatch |
| POST | `/v1/webhooks/inbound` | Generic inbound (JSON or raw MIME) |
| POST | `/v1/webhooks/cloudflare` | Cloudflare Email Routing |
| GET | `/v1/realtime/ws?mailbox_id=` | WebSocket |
| GET | `/v1/stats` | Counts |

### Examples

```bash
# Create domain
curl -X POST http://localhost:8095/v1/domains -H "content-type: application/json" -d '{"domain":"example.com"}'

# Create mailbox
curl -X POST http://localhost:8095/v1/mailboxes -H "content-type: application/json" -d '{"address":"hello@example.com"}'

# Send
curl -X POST http://localhost:8095/v1/send -H "content-type: application/json" -d '{
  "from":"hello@example.com","to":["user@gmail.com"],"subject":"Hello","text":"Hi from Aivory Mail"
}'

# Inbound (simulate)
curl -X POST http://localhost:8095/v1/webhooks/inbound -H "content-type: application/json" -d '{
  "from":"customer@acme.com","to":"hello@example.com","subject":"Invoice #4821 overdue","text":"Invoice #4821 AED 18,500 due 12 days ago"
}'

# Intelligence
curl -X POST http://localhost:8095/v1/intelligence/analyze -H "content-type: application/json" -d '{
  "subject":"Invoice #4821 overdue","body":"Customer ABC Trading AED 18,500"
}'
# → { intent: "invoice", urgency: "High", entities: [{kind:"amount", value:"AED 18,500"}], suggested_actions: [...] }

# WebSocket
wscat -c "ws://localhost:8095/v1/realtime/ws?mailbox_id=<uuid>"
```

---

## Cloudflare deploy

```bash
# 1. Set env in worker/wrangler.jsonc or .dev.vars
# 2. Deploy worker
cd worker && npx wrangler deploy

# 3. Enable Email Routing in Cloudflare dashboard:
#    Zone → Email → Email Routing → Enable → Add route hello@example.com → Worker aivory-mail-worker

# 4. Point Aivory Mail API to your deployment
#    AIVORY_MAIL_API_URL=https://mail.aivory.id
#    INTERNAL_TOKEN=...
```

---

## VPS deploy

```bash
# On VPS with Aivory V2:
docker network create aivory-network 2>/dev/null || true
docker compose up -d --build
# Traefik will route mail.aivory.id → :8095 (see docker-compose.yml labels)
# Point MX → VPS IP, or use Cloudflare Email Routing → Worker → VPS
```

Point DNS:

```
MX  @  mail.aivory.id  (or Cloudflare Email Routing)
TXT @  v=spf1 include:_spf.mx.cloudflare.net ~all
TXT _dmarc  v=DMARC1; p=quarantine;
```

---

## Cerveau / Aivory Intelligence

- Heuristic intelligence runs offline (intent, urgency, entities, suggested actions).
- If `AI_GATEWAY_URL` is set, `POST /v1/intelligence/analyze` also calls `AI_GATEWAY_URL/v1/ai/analyze-email` and merges.
- Inbound triggers `WORKFLOW_URL/webhook/email-received` and `AI_GATEWAY_URL/v1/mail/intelligence` async.

Compatible with:
- `services/cerveau` (skills) — map `mail.intelligence` events to workflow
- `avry-zeroclaw` (AI gateway) — `ZEROCLAW_URL`
- `avry-n8n` — `N8N_AS_CODE_URL`

### Email → Business operation

```
Email: "Invoice #4821 overdue — AED 18,500 — ABC Trading"
  → intelligence { intent: invoice, urgency: High, entities: [invoice #4821, AED 18,500, ABC Trading] }
  → suggested_actions [create_task(finance), draft_reply, update_crm]
  → workflow creates Finance Task → notifies → waits approval → sends reminder
```

---

## Testing

```bash
cargo test
cargo check
# live test
DATABASE_URL=sqlite::memory: cargo run --bin aivory-mail-api &
sleep 2
curl -s http://localhost:8095/health | jq .
curl -s http://localhost:8095/v1/stats | jq .
```

---

## License

Proprietary — Aivory © 2026. Cloudflare Email Routing/Sending used as infrastructure layer (limits & acceptable-use apply; per-message SaaS markup eliminated, not all email costs).

## Credits

Inspired by [Mailflare](https://github.com/hieunc229/mailflare) (Cloudflare-native) and [mailcow-dockerized](https://github.com/mailcow/mailcow-dockerized) (production hardening). Core reimplemented in Rust; no direct code copy (Mailflare license forbids commercial SaaS derivative).

---

## Roadmap

- [ ] DKIM signing (lettre dkim) + SPF/DMARC verification
- [ ] Sieve filtering
- [ ] Full MIME attachment inline (mixed multipart)
- [ ] S3/R2 presigned URLs + streaming downloads
- [ ] D1 HTTP binding for Cloudflare Workers
- [ ] Admin UI: domain onboarding wizard (DNS check)
- [ ] Audit log + retention policies
