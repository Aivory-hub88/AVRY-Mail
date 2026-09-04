# Aivory Mail — Progress, Config & Routing

**Repo:** https://github.com/Aivory-hub88/AVRY-Mail  
**Branch:** `main` — VPS `ubuntu@129.226.155.216:63222` (`~/.ssh/claude_code_vps`) `aivory-network` `traefik:v2.11`  
**Domain:** `mail.aivory.uk` (Cloudflare `aivory.uk` zone `518089ea559912f70a3a2911d0cf09af`)

## 1. Deploy & Infra

- **VPS:** Tencent `129.226.155.216` (`22` blocked, `63222` ssh, `avry-postgres:5432` `aivory_mail` `aivory:AivoryApp2026!@#123`, `JWT_SECRET=be44f...`, `INTERNAL_TOKEN=5367...`)
- **Images:** `avry-mail:8095` (Rust `bookworm` `lettre 0.11.23` `mail-send 0.4.9`), `avry-mail-web:3005` (Next.js), `avry-mail-smtp:2525/2587` (inbound ingress)
- **Compose:** `docker-compose.mail-prod.yml` (healthcheck `curl /health`, `traefik` `Host(mail.aivory.uk) && (PathPrefix(/v1/)||/health||/mcp)`)
- **Build fixes:** `Dockerfile:2` `FROM rust:bookworm`, `tower-http` `RequestBodyLimitLayer 100MB` + `DefaultBodyLimit::disable()` (`main.rs:69`), `mail-builder 0.3` + `rustls aws-lc-rs` `CryptoProvider` (`main.rs:16`).

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
CF_EMAIL_WORKER_NAME=aivory-mail-worker WORKER_SEND_URL=https://worker.aivory.uk/send
```

## 3. DNS (Cloudflare, proxied:false for mail)

- `A mail.aivory.uk 129.226.155.216` `proxied:false` (`39afc637`) — sebelumnya `proxied:true` -> `104.21.15.183` bikin `MX` fallback `_dc-mx`
- `MX aivory.uk 10 mail.aivory.uk.` (`cd28c85c`) — `dig @arely MX` sekarang `10 mail.aivory.uk.` (sebelum `_dc-mx.ad458...`)
- `TXT aivory.uk v=spf1 include:_spf.aivory.uk ~all`, `TXT _spf.aivory.uk v=spf1 ip4:129.226.155.216 include:spf.mailersend.net include:relay.mailchannels.net ~all` (`c6d2d...`)
- `TXT aivory._domainkey.aivory.uk v=DKIM1; k=rsa; p=MIIBIjAN...`
- `TXT _dmarc.aivory.uk v=DMARC1; p=quarantine; rua=mailto:dmarc@aivory.uk` (`63b5d02`, sebelumnya `postmaster@`)

## 4. DB & Migrations

- `mailboxes` 4: `hello@aivory.uk a44b...`, `irfan.reichmann@aivory.uk d163...`, `career@aivory.uk 6c40...`, `hello@demo.aivory.uk c2f9...` (`is_catch_all` `hello@aivory.uk=true`)
- `domains` 2: `aivory.uk 6da68... Active`, `demo.aivory.uk 6a900... Active`
- `mailbox_aliases` `TEXT` (`domain_id`, `mailbox_id` `TEXT` vs `UUID` -> join `::uuid` di `routing.rs:50`)
- `messages` 229 (`Inbox 146` `Newsletter 38` `Notification 25` `Sent 19` `Drafts 1`), `threads`, `attachments` via `object_store` `local`
- `mail_filters`, `webhooks`, `agent_tasks`, `contacts`, etc.

## 5. Routing — 3-Phase Parity

**`crates/aivory-mail-api/src/mail/routing.rs:23` `resolve_recipient(to)`**
1. `lower(address)=$1` exact `mailboxes`
2. `mailbox_aliases` `ma.mailbox_id::uuid=m.id` + `ma.domain_id::uuid=d.id`
3. `use_all_domains !=0` (`INTEGER` vs `BOOLEAN`)
4. `is_catch_all=true` (`hello@aivory.uk=true`)

**`inbound.rs:31` `handle_inbound_raw_with_folder`**
- `probe = Sent/Drafts ? from : to` -> `resolve_recipient(probe)` + fallback
- `forced_folder` skip `apply_filters`; else `FilterAction::Reject|Block|Forward|Move`
- `snippet` `chars().take(160)` fix `£`/`🚀` (`intelligence.rs:68`), `50MB` limit

**`api/mod.rs:160` `GET /v1/stats?mailbox_id=`** per-mailbox `by_folder`, `web/page.tsx:166,185` per-mailbox fetch + `setMsgs([])` clear.

## 6. API

- `POST /v1/auth/login` `admin@aivory.id` `superadmin` `irfan.reichmann@aivory.uk`
- `GET /v1/messages?folder=...&mailbox_id=` `GET /v1/threads` `GET /v1/stats?mailbox_id=`
- `POST /v1/send` `{from,to,subject,text,html,attachments}` -> `outbound.rs:43` `2MB` `10x10MB` `DKIM` `send_email`
- `POST /v1/webhooks/inbound` `POST /v1/webhooks/cloudflare` `{"from","to","raw":base64,"folder"}` `50MB`
- `POST /v1/ai/ask` `GET /v1/ai/history` `POST /v1/ai/push-to-mission-control`

## 7. Outbound (`outbound.rs:92`)

Urutan `worker-http -> mailchannels -> cloudflare -> smtp (mail_send)`:
- `send_via_worker_http` `WORKER_SEND_URL=https://worker.aivory.uk/send` (`worker/worker.js:15` `EmailMessage` + `SEND_EMAIL` `wrangler.jsonc`)
- `send_via_mailchannels` `https://api.mailchannels.net/tx/v1/send` (`401` saat ini)
- `send_via_cloudflare` `https://api.cloudflare.com/.../email/sending/send` (`10001 invalid_request_schema`)
- `send_via_smtp` -> `send_via_mail_send` (`mail_send::SmtpClientBuilder` `smtp.mailersend.net:587` `implicit_tls(false)` `ring`/`aws-lc-rs` `CryptoProvider` `main.rs:16`) -> `250 Message queued` sukses (terbukti `python smtplib` juga)

`store_sent_message` `INSERT ... VALUES ($1..$12,'Sent',true,false,0,$13,NOW())` (`has_att` fix).

## 8. Frontend (`web/app/page.tsx`)

- `Inbox`/`Sent`/`Drafts`/`Snoozed`/`Archive`/`Spam`/`Trash` + `Newsletter`/`Notification`, `mailbox` selector `selectedMailboxId`
- `AskAIAssistant.tsx` floating `fixed bottom-6 right-6`

## 9. Worker (`worker/worker.js`, `wrangler.jsonc`)

- `email(message, env, ctx)` forward `POST ${AIVORY_MAIL_API_URL}/v1/webhooks/cloudflare` (`x-internal-token`)
- `fetch /send` outbound via `SEND_EMAIL` + fallback `mailchannels`
- Deploy via `PUT /accounts/5cc9dc6b.../workers/scripts/aivory-mail-worker` (bypass `wrangler` `memberships`), routes `mail.aivory.uk/send*` + `worker.aivory.uk/*` (`c7e332...`, `7081b0...`)

## 10. Migration Zoho -> AVRY

- `229` EML via `import_final.py` `POST /v1/webhooks/inbound` `folder` + `email.parser` + `50MB` -> `229/229` `Inbox 146` `Sent 19` etc.

## 11. Next / TODO

- Verifikasi `Email Routing Addresses` `hello@`/`irfan@` (klik link di inbox) biar `SEND_EMAIL` `destination not verified` hilang.
- `build_message` `html` + `attachments` `multipart` masih `lettre` `InvalidContentType` untuk complex — sudah work untuk `text/plain` via `mail_send`.
