# Aivory Mail + Mail Assistant — Update 2026-09-07

> Repo: https://github.com/Aivory-hub88/AVRY-Mail (branch `main`) — submodule `services/avry-mail` di `AVRY-V2-Main`
> VPS: `tencent-vps` / `aivory-prod` — 129.226.155.216:63222 (`ubuntu` / `irfan`), `aivory-network`
> Stack: Rust (axum/sqlx) + Next.js 15 + Postgres (`aivory_mail`) + Dovecot 993/587 + Redis + Qdrant + Cognee-RS + Zeroclaw vanilla

---

## Ringkasan

Update ini menuntaskan 4 pekerjaan utama yang diminta berturut-turut:

1. **Pisah password Web vs IMAP** — dua kredensial benar-benar independen (Gmail App-passwords parity).
2. **Self-service IMAP untuk semua user** — `Settings > Integrations • Account • IMAP` dengan `host/port/username/password` (masked + Test) dan `Connected as …` yang tidak pernah render password lagi; admin tetap bisa cek status.
3. **Keamanan AI Assistant** — isolasi per-mailbox (tidak bocor cross-user) + kasih “mata” Sent/delivery.
4. **Jalur Mail ↔ Cerveau** — bridge per-mailbox isolated + wiring Redis / Qdrant / Cognee-RS / Zeroclaw vanilla.

Semua perubahan sudah di-push ke `AVRY-Mail` main dan live di VPS (containers healthy, `openapi.json` regenerate).

---

## 1. Pisah Generate Password — Web vs IMAP

### Masalah
Admin console sebelumnya pakai satu field password untuk web login **dan** IMAP (`password_hash` + `password_hash_dovecot` diisi barengan). Ganti satu = jebol yang lain.

### Solusi
- **DB:** `mailboxes.password_hash` (web, stretched SHA-256) dan `mailboxes.password_hash_dovecot` (`{SHA512}` base64 untuk Dovecot) benar-benar terpisah. Migrasi `014_mailbox_password.sql` + `016_dovecot.sql` + `ensure_schema()` di `crates/aivory-mail-api/src/main.rs:83`.
- **Backend** `crates/aivory-mail-api/src/api/mailboxes.rs:66`:
  - `POST /v1/mailboxes` & `PUT /v1/mailboxes/:id` hanya sentuh `password_hash` (min 8).
  - `POST /v1/mailboxes/:id/imap-password` (auto-generate kalau body kosong, balikin plaintext **sekali**) & `DELETE .../imap-password` (revoke) hanya sentuh `password_hash_dovecot`. Dovecot `dovecot.conf:50` query `WHERE password_hash_dovecot IS NOT NULL`.
- **Admin UI** `web/app/admin/page.tsx:46` — modal IMAP terpisah dengan Generate/Copy, info `mail.aivory.uk:993 SSL / 587 STARTTLS`, revoke tanpa lock webmail.

Commit: `aa27965 feat(admin): split web-login and IMAP passwords`

---

## 2. Self-Service IMAP — Settings > Integrations • Account • IMAP

### Request tim
> “Better IMAP password ini diberikan ke semua user tapi bisa di check juga oleh admin kalau user lupa, jadi ditambahkan di Settings page — tapi bikin sub-section sendiri, misal Settings > Integrations > Email Account, bukan digabung ke profile settings umum. Form-nya: IMAP host, port, username, password (masked input, show/hide toggle), test-connection button sebelum save. Setelah save, jangan pernah render password itu lagi di UI — cuma tampilkan ‘Connected as user@domain.com’ dengan opsi disconnect/reconnect.”

Iterasi UX:
- Awalnya bikin halaman terpisah `/settings/integrations` (`6329341`), lalu user minta **jangan lompat** → embed langsung di `Mail user settings` sebagai tab (`0ed7ba8`), lalu **pindah tepat di bawah General** (`abc30ff`), lalu **rename** jadi `Integrations • Account • IMAP` (`284347c`).

### Backend
- **Migrasi** `migrations/017_email_integrations.sql` — tabel `email_integrations (id, mailbox_id UNIQUE, host, port, username, status, last_tested_at, last_connected_at)` + `ensure_schema()` di `main.rs`.
- **API** `crates/aivory-mail-api/src/api/integrations.rs:1`:
  | Method | Path | Auth | Deskripsi |
  |---|---|---|---|
  | GET | `/v1/integrations/email` | JWT own mailbox | Status own: `{host, port, username, status, connected, has_password}` — **tidak pernah balikin password** |
  | POST | `/v1/integrations/email/test` | JWT | Validasi + TCP-probe `host:port` tanpa save |
  | POST | `/v1/integrations/email` | JWT own | Save & hash `password → password_hash_dovecot` + upsert `email_integrations` `status=connected` |
  | DELETE | `/v1/integrations/email` | JWT own | Disconnect — clear `password_hash_dovecot`, `status=disconnected` (web tetap jalan) |
  | GET | `/v1/integrations/email/admin?mailbox_id=` | Admin | Cek status mailbox mana pun tanpa expose secret (untuk “user lupa”) |
- **Komposisi:** `docker-compose.mail-prod.yml` env `COGNEE_URL`/`REDIS_URL` dll sudah, `openapi.json` 91 paths, `docs/API.md` section baru.

### Frontend
- **Lokasi:** `web/app/settings/integrations/page.tsx` (direct) **dan** `web/app/settings/mail/page.tsx:16` `TABS` baru `integrations` tepat di bawah `general`, panel `tab==="integrations"` (`mail/page.tsx:120`):
  - Grid `host` (`mail.aivory.uk`) / `port` (`993`) / `username` (prefill alamat), `password` masked + `Show/Hide`, `Test connection` (wajib `testOk===true` sebelum Save), `Save & connect` → switch ke **Connected as …** `host:port` + `Disconnect`/`Reconnect`. Info client `IMAP 993 SSL · SMTP 587 STARTTLS`.
- **Discoverability:** `web/app/settings/page.tsx` tombol `Integrations • Account • IMAP` di Overview, `mail/page.tsx` sebelumnya sempat banner `→ Integrations…` lalu dihapus setelah embed.
- **Admin:** `web/app/admin/page.tsx:276` kolom baru **Email integration** per row (`Connected as … · host:port` / `Disconnected`) via `GET /v1/integrations/email/admin` loop di `loadAll()`.

Commit: `6329341`, `9affe61`, `0ed7ba8`, `abc30ff`, `284347c`

---

## 3. Keamanan AI Assistant — Bocor Cross-User & “Buta” Sent

### Laporan
> “AI assistant masih bocor ke user lain, dan chat history juga bocor” — screenshot `Ringkas inbox hari ini` nampilin `240 total / 85 unread` dari mailbox lain.
>
> “AI belum punya mata dan telinga untuk check delivery status … ‘check the last email i sent. is it delivered?’ → ‘no visibility into your Sent folder’”

### Akar masalah
- `crates/aivory-mail-api/src/api/ai_chat.rs:42` `ask` dan `121` `history` **tanpa auth**, `mailbox_id` dari body/query bebas. `history` tanpa `mailbox_id` malah `SELECT ... ORDER BY ... LIMIT` semua user. `fetch_overview` sebelumnya sum semua mailbox kalau `mailbox_id` kosong.
- Prompt `crates/aivory-mail-core/src/email_assistant.rs:1` cuma kirim `Inbox overview`, nggak ada Sent, jadi AI jujur bilang buta Sent.

### Fix isolasi (`1a58724`)
- `ai_chat.rs:16` `own_mailbox_id()` dari `Authorization: Bearer <JWT>` (`authz::authenticated_email` → `SELECT id FROM mailboxes WHERE lower(address)=lower(email)`). `ask` & `history` sekarang wajib JWT, `mailbox_id` harus `== own_mid` kecuali admin (`is_admin` cek `MAIL_ADMIN_EMAIL`/`SUPERADMIN_EMAIL`/`domains.admin_email`). `user_email` dari JWT, bukan body. `history` non-admin tanpa `mailbox_id` → force ke own, coba id lain → 403. Frontend `web/components/AskAIAssistant.tsx:26` sekarang kirim `Authorization` untuk history dan skip kalau `mailboxId` kosong.
- Verifikasi VPS: `curl /v1/ai/history` tanpa auth → **401**, `POST /v1/ai/ask` tanpa auth → 401 (sebelumnya 200 dengan dump).

### Fix mata Sent/delivery (`38687a1`)
- `ai_chat.rs:404` `fetch_sent_overview()` per-mailbox: `SELECT COUNT(*) WHERE folder='Sent'` + `last_sent subject/to/created_at` dari `messages`. Dikirim sebagai `Sent overview: sent_total …, last_sent subject '…' to … at … — present in Sent means relay accepted`.
- `email_assistant.rs:4` `SYSTEM_PROMPT` sekarang `Inbox + Sent`, aturan explicit “jangan bilang no visibility — kamu punya Sent overview”, `build_prompt()` bawa `sent_overview`, plus `tools: get_sent_overview`. Heuristic fallback juga kenal `delivered/sent/terkirim`.
- Build prompt: `Question + Inbox overview + Sent overview + Thread memory + Selected message context`.

Hasil: `clement@aivory.uk` tanya `ringkas inbox` sekarang angka milik clement saja; `check last email i sent` dijawab dari Sent miliknya (`sent_total … last_sent …`).

---

## 4. Jalur Mail ↔ Cerveau — Per-Mailbox Isolated Relay

### Desain (setuju hybrid)
Tetap **Zeroclaw vanilla** sebagai fast path; kalau butuh memory dalam, Mail relay ke Cerveau dengan `mailbox_id` yang sama — Cerveau callback via MCP dengan `mailbox_id` yang sama.

### Implementasi
- **Bridge** `crates/aivory-mail-api/src/api/cerveau_relay.rs:1`:
  - `GET /v1/cerveau/agents` (JWT) — list `workflow_*`, `mail_ops`, `mail_memory` (mirror `services/cerveau/skills/*`).
  - `POST /v1/cerveau/ask` (JWT, isolated): `{question, context:{mailbox_id, thread_id}, agent}` → derive `own_mid`, enforce ownership, build payload `{mailbox_id, mailbox_email, mcp:{base, tools}, callback}`, coba `COGNEE_URL` (`/v1/cerveau/ask` / `/invoke` dengan `x-mailbox-id`, `x-cerveau-internal-secret`) → fallback `AI_GATEWAY_URL=http://avry-zeroclaw-daemon:3010` `/v1/ai/chat` → fallback heuristic. Simpan trace `ai_chat_history model=cerveau`. Fix `7c3e526`: admin tanpa mailbox bisa relay kalau kasih `mailbox_id` eksplisit, `COGNEE_URL=""` di-skip (jangan POST ke `/v1/cerveau/ask` relatif).
- **MCP Mail** `crates/aivory-mail-api/src/mcp.rs:33` sekarang **mailbox_id-aware**: `search_mail` & `get_inbox_overview` terima `mailbox_id` opsional dan filter `WHERE mailbox_id=...` (Postgres/SQLite). `tools/list` advertise `mailbox_id`.
- **Wiring** `crates/aivory-mail-api/src/api/mod.rs:37` daftar `pub mod cerveau_relay`, routes `/v1/cerveau/agents` & `/v1/cerveau/ask`, `openapi.json` 91 paths, `docs/API.md` section baru.

Test VPS: `GET /v1/cerveau/agents` tanpa auth → 401 → dengan JWT admin → 200 (7 agents), `POST /v1/cerveau/ask` untuk `clement@aivory.uk` → `via: heuristic` (daemon mock, tapi header `x-mailbox-id` terkirim).

---

## 5. Wiring Redis / Qdrant / Cognee-RS / Zeroclaw

### Redis
- `redis` Up 2 weeks `aivory-network` `172.18.0.10`, `REDIS_PASSWORD=***`, `AVRY-Mail/.env` `REDIS_URL=redis://:***@redis:6379/0` + `docker-compose.mail-prod.yml` env `REDIS_URL`/`AI_GATEWAY_URL`/`OPENROUTER_API_KEY` (via `env_file` + `environment`). `docker exec redis redis-cli -a *** ping` → `PONG`, `avry-mail env` keisi. Code Mail belum pakai client Redis (grep nol) — siap untuk cache/ratelimit.

### Qdrant + Cognee-RS
- `qdrant/qdrant:v1.9.0` & `cognee/cognee:latest` (2.35GB) dipull. `docker-compose.mail-prod.yml` ditambah service `qdrant` (6333, `qdrant_storage`) & `cognee-cerveau` (8000:8000, `ENV=production`, `DB_PROVIDER=sqlite` — awalnya coba postgres tapi fail `invalid interpolation` karena `AivoryApp2026!@#123`, switch ke sqlite), `depends_on: qdrant`. Healthcheck di-fix dari `curl` ke `bash tcp` + `wget`. Sekarang **both healthy**: `qdrant Up healthy` `6333`, `cognee-cerveau Up healthy` `0.0.0.0:8000` `{"status":"ready","health":"healthy","version":"1.5.4-local"}`, `docker exec avry-mail curl http://cognee-cerveau:8000/health` → ready. `AVRY-Mail/.env` `COGNEE_URL=http://cognee-cerveau:8000` (sebelumnya 3200 clash dengan `cerveau-server` di host 3200).

### Zeroclaw vanilla
- `services/avry-zeroclaw` submodule di-init di `avry-v2-main-src` (copy `bin/`+`Dockerfile.zeroclaw` dari `AVRY-V2-Main`), build `avry-zeroclaw-daemon` sukses, `docker-compose.production.yml` di-wire `COGNEE_URL`, `REDIS_URL`, `INTERNAL_TOKEN`, `OPENROUTER_API_KEY`. Container `avry-zeroclaw-daemon` Up (mock, `health: starting` tapi `3010/tcp` jalan, `redis:6379` & `cognee:8000` reachable via `bash tcp`).

Semua di `aivory-network`, `avry-mail` → `cognee:8000` & `zeroclaw:3010` & `redis:6379` reachable.

---

## Deployment & Verifikasi

- **Build & push:** `cargo check` & `next build` lolos (`/settings/integrations` 3.93kB, `/settings/mail` 9.21kB). `python3 scripts/gen_openapi.py` → 91 paths. Push ke `Aivory-hub88/AVRY-Mail` main: `1e23da5` (folder switch), `284347c` (rename), `1a58724` (isolasi), `38687a1` (Sent), `a4ba223`+`7c3e526` (bridge), `abc30ff`/`9affe61`/`0ed7ba8`/`6329341`/`aa27965` sebelumnya. `AVRY-V2-Main` bump `9836677` untuk `services/avry-mail`.
- **VPS pull & rebuild:** `git pull --ff-only` di `~/AVRY-Mail`, `docker compose -f docker-compose.mail-prod.yml build avry-mail` + `up -d` (avry-mail healthy, avry-mail-web healthy, qdrant healthy, cognee-cerveau healthy, redis Up, dovecot Up). `docker compose -f avry-v2-main-src/docker-compose.production.yml build/up avry-zeroclaw-daemon`.
- **Health:** `curl http://localhost:8095/health` → `{"db":"connected","status":"ok"}`, `GET /v1/cerveau/agents` 401→200, `POST /v1/cerveau/ask` 401→ heuristic OK, `GET /v1/integrations/email` 401→ ok, `GET /settings/integrations` 200, `mail:200`.

---

## Cara Pakai

- **User:** `https://mail.aivory.uk` → `Settings / Mail` → tab **General** → **Integrations • Account • IMAP** (urutan ke-2) → isi `host`/`port`/`username`/`password` (Show/Hide) → **Test connection** → **Save & connect** → jadi `Connected as you@domain.com · host:port` + `Disconnect`/`Reconnect`. Password nggak pernah dirender lagi. Direct juga `https://mail.aivory.uk/settings/integrations`.
- **Admin:** `https://mail.aivory.uk/admin` → `Users` → kolom **Email integration** (Connected/Disconnected + `host:port`) + tombol `IMAP password` (issue/revoke). Cek status user lupa via `GET /v1/integrations/email/admin?mailbox_id=`.
- **AI:** `Ask AI Assistant` di inbox — sekarang tanya `check the last email i sent. is it delivered?` dijawab dari Sent milik sendiri (`sent_total … last_sent …`), bukan “no visibility”.

---

## File Penting

- Backend: `crates/aivory-mail-api/src/api/mailboxes.rs`, `integrations.rs`, `ai_chat.rs`, `cerveau_relay.rs`, `mcp.rs`, `cognee.rs`, `main.rs` (`ensure_schema`), `config.rs`; `migrations/017_email_integrations.sql`; `crates/aivory-mail-core/src/email_assistant.rs`
- Frontend: `web/app/settings/integrations/page.tsx`, `web/app/settings/mail/page.tsx`, `web/components/AskAIAssistant.tsx`, `web/app/page.tsx` (`goToFolder` → `setActiveTab("mail")`), `web/app/admin/page.tsx`, `web/app/settings/page.tsx`
- Infra: `docker-compose.mail-prod.yml` (avry-mail + qdrant + cognee-cerveau), `docker-compose.production.yml` (avry-zeroclaw-daemon), `.env` (`COGNEE_URL`, `REDIS_URL`, `AI_GATEWAY_URL`)
- Docs: `docs/API.md`, `docs/openapi.json`, `docs/ARCHITECTURE.md`

---

## Next (opsional)

- Cognee ingest: `cognee` masih sqlite — kalau mau postgres + `qdrant` vector, set `DB_PROVIDER=postgres` dengan URL-encode password yang benar atau pakai `DATABASE_URL` terpisah.
- Zeroclaw real daemon: sekarang mock — ganti `bin/zeroclaw` dengan binary Rust asli biar `health` jadi healthy bukan starting.
- Redis usage: wiring sudah, tinggal pakai di rate-limit / cache AI.

