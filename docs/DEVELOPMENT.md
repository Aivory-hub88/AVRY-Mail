# Aivory Mail — Development

Local development notes for the whole stack: **Rust API**, **Next.js web**,
**SMTP ingress**, migrations, and common gotchas.

## Ports

| Service               | Port | Run from                     |
|-----------------------|------|------------------------------|
| Rust API (axum)       | 8095 | repo root                    |
| Next.js web (dev)     | 3005 | `web/`                       |
| SMTP ingress (VPS)    | 2525 | `crates/aivory-mail-smtp`    |
| SMTP submission       | 2587 | `crates/aivory-mail-smtp`    |
| Postgres (docker)     | 5436 | docker-compose `avry-mail-db`|

## Prerequisites

- Rust toolchain (`rust-toolchain.toml` pins the version)
- Node 20+ (web uses Next 15)
- Either a local Postgres **or** SQLite (default for quick dev)

## 1. Rust API

### SQLite quickstart (zero setup)

```bash
cd <repo-root>
cp .env.example .env
# .env:
#   PORT=8095
#   DATABASE_URL=sqlite://./data/mail.db
#   STORAGE_BACKEND=local
#   STORAGE_PATH=./data/mail-storage
#   MAIL_MODE=vps

cargo run --bin aivory-mail-api
# → http://localhost:8095/health
```

### Postgres (docker)

```bash
docker compose up -d avry-mail-db     # Postgres on :5436
DATABASE_URL=postgresql://postgres:postgres@localhost:5436/aivory_mail \
  cargo run --bin aivory-mail-api
```

### Verify

```bash
curl -s http://localhost:8095/health | jq .
curl -s http://localhost:8095/v1/stats | jq .
```

## 2. Next.js web

```bash
cd web
npm install --legacy-peer-deps   # next@15 peer-dep on react 19 rc — required
npm run dev                      # → http://localhost:3005
```

> **Note:** `next@15.0.0` declares `peer react@^18.2.0 || 19.0.0-rc-…`.
> That conflicts with `react@19.0.0` on npm ≥ 7 strict resolution, so always
> install with `--legacy-peer-deps`.

The web calls the API at `NEXT_PUBLIC_MAIL_API` (default `http://localhost:8095`):

```bash
NEXT_PUBLIC_MAIL_API=http://localhost:8095 npm run dev
```

### Web routes

- `/` — Inbox (list, thread view, compose, star, share, signature modal)
- `/calendar` — calendar (Google-parity week view)
- `/settings` — API keys + Remote MCP
- `/settings/mail` — user settings (10 Gmail/Zoho/Outlook parity tabs)
- `/share/[id]?t=<token>` — public read-only shared message

## 3. SMTP ingress (VPS mode)

```bash
AIVORY_MAIL_API_URL=http://localhost:8095 \
SMTP_INGRESS_PORT=2525 \
INTERNAL_TOKEN=aivory-internal-dev \
cargo run --bin aivory-mail-smtp
```

Deliver to `127.0.0.1:2525` from any MTA/`sendmail` to simulate inbound mail.

## 4. Migrations

Migrations are plain SQL under `migrations/`, compatible with both Postgres and
SQLite. They are executed automatically at boot (`DbPool::migrate`), with an
additional idempotent `ensure_schema()` bootstrap for SQLite.

To run them manually:

```bash
sqlx migrate run          # against $DATABASE_URL
```

When adding a schema change:

1. Add `00X_name.sql` (numbered) — write both `$1`/`?` variants if the query
   is in a handler, not the migration.
2. Keep migrations idempotent (`CREATE TABLE IF NOT EXISTS`, guarded `ALTER`).
3. If the table is queryable via API, add the CRUD handler in
   `crates/aivory-mail-api/src/api/<name>.rs` and register routes in
   `api/mod.rs`.

## 5. Env reference

See [`.env.example`](../.env.example) for the full list. Highlights:

| Var | Default | Purpose |
|-----|---------|---------|
| `PORT` | `8095` | API port |
| `DATABASE_URL` | `sqlite::memory:` | Postgres or SQLite URL |
| `STORAGE_BACKEND` | `local` | `local` / `r2` / `s3` |
| `STORAGE_PATH` | `./data/mail-storage` | local raw/attachment blob dir |
| `MAIL_MODE` | `vps` | `vps` / `cloudflare` / `hybrid` |
| `JWT_SECRET` | dev default | share-token signing |
| `INTERNAL_TOKEN` | dev default | internal/MCP bypass |
| `CORS_ORIGINS` | localhost 9000/9001 | comma-separated allowlist |
| `COGNEE_URL` | — | Cerveau sidecar for knowledge compile |
| `NEXT_PUBLIC_MAIL_API` (web) | `http://localhost:8095` | API base for web UI |

## 6. Common gotchas

- **API key reveal mismatch** — masked + raw come from the same `key_raw`
  column since `2e0a719`; if a key predates that, re-create it so reveal is
  consistent (`avry-…` prefix, hash matches raw).
- **ERESOLVE on `npm install`** — always `--legacy-peer-deps` (see web above).
- **`sqlite::memory:` default** — data is lost on restart; use a file URL for
  persistent dev.
- **SQLite migrations** — `ensure_schema()` in `main.rs` covers tables; but to
  add columns update both migration and `ensures_schema` `alters` list.

## 7. Tests

```bash
cargo test                      # rust unit/integration
cargo check                     # quick compile check
make check                      # same
```

Web has no test command wired yet (`next lint` only).