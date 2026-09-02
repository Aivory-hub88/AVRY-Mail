# Aivory Mail — Agent Guide (read first)

> Ini adalah sumber context utama bagi siapa pun / agent AI apa pun yang bekerja di
> repo ini, di IDE mana pun (VS Code/Cline, Cursor, Claude Code, Copilot, dsb).
> Baca file ini sebelum menyentuh kode — jalankan perintah dari repo ini, bukan dari
> salinan lain.

## Identity

| | |
|---|---|
| **Repo permanen** | `/Users/ireichmann/Documents/AVRY-Mail` |
| **Remote** | `https://github.com/Aivory-hub88/AVRY-Mail.git` (branch `main`) |
| **Posisi di Aivory V2** | git submodule `services/avry-mail` (`.gitmodules`) |
| **Stack** | Rust (axum, sqlx, tokio) + Next.js 15 (App Router, Tailwind v4) |
| **Baru selesai** | `0805ba4` docs + user settings 10 kategori, API key reveal konsisten |

> ⚠️ **JANGAN** kerja di `/private/tmp/avry-mail` (volatile) atau
> `/Users/ireichmann/Documents/Aivory V2/services/avry-mail/*` selain lewat submodule.
> Satu-satunya working copy yang benar adalah repo ini.

## TL;DR — run the stack

```bash
# API (Rust) — port 8095, sqlite di ./data/mail.db
cd /Users/ireichmann/Documents/AVRY-Mail
./scripts/dev-local.sh          # start API + web sekaligus
./scripts/stop-local.sh         # stop keduanya

# manual:
#   cargo run --bin aivory-mail-api            # backend
#   cd web && npm run dev                       # frontend :3005
```

- API: `http://localhost:8095` · `/health` · `/v1/stats`
- Web: `http://localhost:3005` · `/` (inbox), `/settings`, `/settings/mail`, `/calendar`
- SMTP ingress: `:2525` (hanya di VPS mode docker)

## Docs (source of truth)

| File | Isi |
|------|-----|
| `docs/ARCHITECTURE.md` | System map, crates, mail flow inbound/outbound, DB, AI/MCP |
| `docs/DEVELOPMENT.md` | Setup lokal, env vars, migrations, gotchas, runbook mesin lokal |
| `docs/API.md` | Referensi endpoint lengkap |
| `docs/USER_SETTINGS.md` | 10 kategori user settings Gmail/Zoho/Outlook + status |
| `docs/DEPLOYMENT.md` | Docker/VPS, Cloudflare Worker, DNS, submodule |
| `docs/openapi.json` | OpenAPI spec — **dijaga** oleh `scripts/gen_openapi.py` |

Regenerate OpenAPI setelah ada route baru:

```bash
python3 scripts/gen_openapi.py
```

## Stack & layout

```
crates/
  aivory-mail-core/     # types, MIME parser, routing, intelligence heuristics
  aivory-mail-storage/  # DbPool (Postgres/SQLite) + ObjectStore (local/R2/S3)
  aivory-mail-api/      # Axum API + realtime + handlers   ← mayoritas kerja ada di sini
  aivory-mail-smtp/     # SMTP ingress (VPS)
web/
  app/                  # Next.js UI (inbox, calendar, settings)
migrations/             # 6 migrations (Postgres + SQLite compatible)
worker/                 # Cloudflare Worker shim
scripts/                # dev-local.sh, stop-local.sh, gen_openapi.py, deploy-vps.sh
```

## Status (kesinambungan kerja)

Selesai & live:
- API keys Tavily-style + reveal konsisten (`avry-…****…` dari `key_raw`).
- **Custom domains beneran jalan**: DNS ownership verification (TXT lookup
  real via hickory-resolver), DKIM keypair per domain + outbound signing
  (mail-auth), live MX/SPF/DKIM/DMARC checklist (`aivory_mail_core::dns` +
  `mail::dns_check`), SMTP ingress nolak RCPT TO untuk mailbox yang gak ada
  (`mail::routing::resolve_recipient` + `/v1/internal/resolve-recipient`),
  `/domains` web UI. Lihat migration `007_dkim_verification.sql`.
- **User settings yang benar-benar diterapkan** (bukan cuma KV tersimpan):
  undo send (delay `/v1/send` beneran di `ComposeModal.tsx`), density,
  conversation view (grouping via `/v1/threads`), filters (route inbound
  mail ke folder via `aivory_mail_core::filters` + `inbound.rs`), vacation
  auto-reply (dedup per sender per `interval_days`, migration
  `008_vacation_dedup.sql`), send-as aliases (`/v1/send-as`, dropdown From
  di compose). Detail lengkap + apa yang masih KV-only: `docs/USER_SETTINGS.md`.
- Nav Settings/Domains sekarang ada di sidebar inbox utama (sebelumnya cuma
  bisa diakses lewat URL langsung).
- Calendar CRUD + conferencing, signatures multi-per-mailbox, share link JWT.

Belum / next candidates (lihat tabel status di `docs/USER_SETTINGS.md`):
- Inbox type/categories, appearance theme/reading-pane, notifications,
  shortcuts, storage — masih tersimpan tapi belum diterapkan ke UI.
- Forwarding rules table (API/UI pakai KV dulu, belum benar-benar forward).
- STARTTLS di SMTP ingress, direct-to-MX outbound sending (lihat roadmap README).

## Conventions

- **Response API**: envelope `{ success: bool }`, payload di `data`, error di `error`.
- **Dual backend SQL**: tiap query ditulis 2× — `$1` (Postgres) & `?` (SQLite) — di handler yang sama.
- **ID**: UUID (Postgres `UUID`, SQLite `TEXT`), timestamp ISO-8601 rfc3339.
- **Frontend**: Next.js App Router, client components (`"use client"`), fetch ke
  `process.env.NEXT_PUBLIC_MAIL_API || "http://localhost:8095"`, Tailwind v4,
  font **Manrope**.
- **Web install**: selalu `npm install --legacy-peer-deps` (next@15 vs react@19 peer conflict).

## Guardrails

- `.env`, `data/`, `target/`, `node_modules/`, `web/.next/` **gitignored** — jangan di-commit.
- Secret (JWT, INTERNAL_TOKEN, API keys, CF token) tidak pernah masuk git.
- Sebelum ubah schema: tambah migration + `main.rs::ensure_schema` + handler di `api/<name>.rs` + route di `api/mod.rs`.
- Setelah ubah route: jalankan `scripts/gen_openapi.py` + update `docs/API.md`.
- Build check: `cargo check` · `cargo test`. Web: `npm run build`.