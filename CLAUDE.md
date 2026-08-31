# CLAUDE.md — Aivory Mail

> Pointer cepat untuk Claude Code. Instruksi lengkap ada di **`AGENTS.md`** (baca dulu).

Bekerja di repo permanen: `/Users/ireichmann/Documents/AVRY-Mail`
(remote: `Aivory-hub88/AVRY-Mail.git`, branch `main`; submodule `services/avry-mail` di Aivory V2).

## Start stack

```bash
./scripts/dev-local.sh    # API :8095 + web :3005
./scripts/stop-local.sh
```

Docs: `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, `docs/API.md`,
`docs/USER_SETTINGS.md`, `docs/DEPLOYMENT.md`, `docs/openapi.json`.

Singkat:
- Backend Rust (axum) port 8095, sqlite `./data/mail.db` (dual Postgres/SQLite).
- Frontend Next.js 15 port 3005 — install selalu `npm install --legacy-peer-deps`.
- Jangan kerja di `/private/tmp/avry-mail` atau folder `Aivory V2/services/avry-mail` di luar submodule.
- Secret/env vars tidak di-commit. Regenerate spec: `python3 scripts/gen_openapi.py`.