# Aivory Mail — Deployment

Covers: Docker compose (VPS), Cloudflare Worker (Email Routing), DNS,
verification, and the repo/submodule workflow.

## 1. Docker compose (VPS)

`docker-compose.yml` starts three services:

| Service          | Container          | Ports                          | Purpose                        |
|------------------|--------------------|--------------------------------|--------------------------------|
| API              | `avry-mail`        | `8095:8095`                    | Axum API + web backend         |
| DB               | `avry-mail-db`     | `5436:5432` (Postgres 16)      | `aivory_mail` database         |
| SMTP ingress     | `avry-mail-smtp`   | `2525:2525`, `2587:2587`       | inbound SMTP → API             |

Compose expects an external docker network `aivory-network` (shared with the
rest of the Aivory stack) and Traefik for TLS:

```bash
docker network create aivory-network 2>/dev/null || true
docker compose up -d --build
curl -s http://localhost:8095/health | jq .
```

### Traefik labels (built in)

```yaml
traefik.http.routers.avry-mail.rule = Host(`mail.aivory.uk`)
traefik.http.routers.avry-mail.entrypoints = websecure
traefik.http.routers.avry-mail.tls.certresolver = letsencrypt
traefik.http.services.avry-mail.loadbalancer.server.port = 8095
```

## 2. Cloudflare Email Routing + Worker

`worker/worker.js` is the Cloudflare Worker shim: it receives emails from
**Email Routing** and forwards to the API via `AIVORY_MAIL_API_URL`.

```bash
cd worker
# set env in wrangler.jsonc or .dev.vars
npx wrangler deploy
```

Then in the Cloudflare dashboard:

1. Zone → **Email** → **Email Routing** → Enable.
2. Add routing rule `hello@example.com` → **Worker** → `aivory-mail-worker`.
3. Point the API at your deployment:
   - `AIVORY_MAIL_API_URL=https://mail.aivory.uk`
   - `INTERNAL_TOKEN=…` (same value in worker + API)

`MAIL_MODE=cloudflare|hybrid` also enables Cloudflare routing-rule creation
when a mailbox is created (`CfClient`), using `CF_API_TOKEN`/`CF_ZONE_ID`.

## 3. DNS records

| Type | Name      | Value                         | Purpose         |
|------|-----------|-------------------------------|-----------------|
| A    | mail      | VPS IP or CF proxy            | Web UI / API    |
| MX   | @         | `mail.aivory.uk` (or CF Email Routing target) | inbound |
| TXT  | @         | `v=spf1 include:_spf.mx.cloudflare.net ~all` | SPF   |
| TXT  | `_dmarc`  | `v=DMARC1; p=quarantine;`    | DMARC           |
| TXT  | `aivory._domainkey` | DKIM (once implemented) | DKIM     |

## 4. Env for prod

Copy `.env.example` → `.env` on the host/VPS and set at minimum:

```
PORT=8095
MAIL_MODE=vps                    # or cloudflare / hybrid
DATABASE_URL=postgresql://postgres:postgres@avry-mail-db:5432/aivory_mail
STORAGE_BACKEND=local            # or r2 / s3
STORAGE_PATH=/app/data/mail-storage
JWT_SECRET=<long random>
INTERNAL_TOKEN=<long random>
CORS_ORIGINS=https://aivory.id,https://dashboard.aivory.id,https://mail.aivory.uk
```

Optional integrations:

```
SMTP_HOST / SMTP_PORT / SMTP_USER / SMTP_PASSWORD   # outbound relay (vps)
CF_API_TOKEN / CF_ZONE_ID / CF_EMAIL_WORKER_NAME    # cloudflare mode
AI_GATEWAY_URL / WORKFLOW_URL                       # Cerveau/ZeroClaw/n8n
COGNEE_URL / COGNEE_INTERNAL_SECRET                 # knowledge sidecar
OPENROUTER_API_KEY                                  # AI models
```

## 5. Health & first checks

```bash
curl -s https://mail.aivory.uk/health | jq .
# → { "status":"ok", "service":"aivory-mail", "mode":"vps", "db":"connected" }

curl -s https://mail.aivory.uk/v1/stats | jq .

# Create a mailbox
curl -s -X POST https://mail.aivory.uk/v1/mailboxes \
  -H 'content-type: application/json' \
  -d '{"address":"hello@example.com"}'
```

## 6. Repo layout in Aivory V2

This repository lives at **`Aivory-hub88/AVRY-Mail`** and is registered as a
git **submodule** under `services/avry-mail` in `Aivory V2` (`.gitmodules`).

```bash
# first clone / update
cd ~/Documents/"Aivory V2"
git submodule update --init services/avry-mail

# work inside the submodule as a normal repo
cd services/avry-mail
git remote -v          # origin → https://github.com/Aivory-hub88/AVRY-Mail.git
git pull origin main
```

> Since `services/avry-mail` used to be an untracked copy, the submodule is the
> canonical way to keep it versioned without leaking into the parent repo.

## 7. Secrets & hygiene

- `key_raw`, `JWT_SECRET`, `INTERNAL_TOKEN`, `CF_API_TOKEN`, `OPENROUTER_API_KEY`
  are **never committed**.
- `.env`, `data/`, `target/`, `node_modules/`, `web/.next/` are gitignored.
- API keys are stored hashed (SHA-256); raw only returned at creation and used
  for the masked/reveal UI.