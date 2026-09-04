# Aivory Mail — Full Configuration

> REDACTED for GitHub — secrets replaced with ***. Full file is ~/AVRY-Mail/.env on VPS 129.226.155.216:63222.

## 1. VPS & Infra
- VPS: ubuntu@129.226.155.216:63222, aivory-network, traefik:v2.11, avry-postgres:5432
- Domain: mail.aivory.uk (Cloudflare aivory.uk zone 518089ea...)
- Images: avry-mail:8095, avry-mail-web:3005, avry-mail-smtp:2525/2587

## 2. Env (~/AVRY-Mail/.env — hybrid)
```
PORT=8095
MAIL_MODE=hybrid
DATABASE_URL=postgresql://aivory:***@avry-postgres:5432/aivory_mail
STORAGE_BACKEND=local
JWT_SECRET=***REDACTED***
INTERNAL_TOKEN=***REDACTED***
MAIL_ADMIN_EMAIL=admin@aivory.id
MAIL_ADMIN_PASSWORD=***REDACTED***
SUPERADMIN_EMAIL=irfan.reichmann@aivory.uk
CF_API_TOKEN=***REDACTED***
CF_ZONE_ID=518089ea...
SMTP_HOST=smtp.mailersend.net SMTP_PORT=587 SMTP_USER=MS_b1vdPa@aivory.uk SMTP_PASSWORD=***REDACTED***
WORKER_SEND_URL=https://worker.aivory.uk/send
OPENROUTER_API_KEY=***REDACTED***
```

## 3. Mailboxes & Domains
- Domains: aivory.uk Active, demo.aivory.uk Active
- Mailboxes: hello@aivory.uk (catch-all), irfan.reichmann@aivory.uk, career@aivory.uk, hello@demo.aivory.uk
- Aliases 13 for hello@: sales@, advisory@, billing@, support@, contact@, info@, admin@, team@, noreply@, postmaster@, dmarc@, abuse@, privacy@
- Zoho legacy: irfan Lemonandsalt..., hello SendToTheWorld..., career VacancyAVRY..., admin Avry786876!@

## 4. DNS (Cloudflare)
- A mail.aivory.uk 129.226.155.216 proxied:false
- MX aivory.uk 10 mail.aivory.uk.
- TXT _spf.aivory.uk v=spf1 ip4:129.226.155.216 include:spf.mailersend.net include:relay.mailchannels.net ~all
- TXT aivory._domainkey v=DKIM1 k=rsa p=MIIBIjAN...
- TXT _dmarc v=DMARC1 p=quarantine rua=mailto:dmarc@aivory.uk

## 5. Routing
- 3-phase: mailbox -> alias ::uuid -> use_all_domains !=0 -> catch-all
- Inbound probe Sent/Drafts via from
- Outbound worker-http -> mailchannels -> cloudflare -> smtp (mail_send)

## 6. Files Private
- ~/AVRY-Mail/.env (never push)
- ~/.ssh/claude_code_vps
