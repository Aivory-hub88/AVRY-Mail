# Aivory Mail Cloudflare Adapter

This directory holds the Cloudflare Worker shim that forwards Email Routing events to the Aivory Mail API.

- `email()` handler → `POST /v1/webhooks/cloudflare` on your Aivory Mail deployment
- `fetch()` handler → proxies dashboard/API (optional)

## Token (required)

The worker and the API share the server's `INTERNAL_TOKEN`, sent as
`x-internal-token`:

- **inbound** — the worker sends it to `POST /v1/webhooks/cloudflare`;
- **outbound** — the API sends it to `POST /send` on the worker, which refuses
  anything else (it is a mail relay for `@aivory.uk`).

Store it as a Worker **secret**. wrangler does not expand `${...}` in
`wrangler.jsonc`, so a value written there is sent literally; the worker
treats an unexpanded placeholder as "not configured" and rejects.

Deploy order (the first step also drops a stale plaintext
`AIVORY_MAIL_API_TOKEN` var, which would otherwise shadow the secret):

```bash
cd worker
npx wrangler login
npx wrangler deploy
# pipe the value so it never lands in your shell history or terminal
ssh aivory-prod 'sudo grep "^INTERNAL_TOKEN=" /home/ubuntu/AVRY-Mail/.env | cut -d= -f2-' \
  | tr -d '\n"' | npx wrangler secret put AIVORY_MAIL_API_TOKEN
npx wrangler secret list   # AIVORY_MAIL_API_TOKEN must be listed
node auth.test.mjs         # sanity check of the /send gate
```

Then confirm the API log shows `webhook cloudflare: internal token ok` for the
next real inbound email. Mail the API rejects makes the worker throw, so
Cloudflare retries and then bounces it to the sender; it is never silently
dropped.

For full Rust Worker (wasm), build `crates/aivory-mail-api` with `worker` feature (requires `worker` crate).
For MVP, use the JS shim in `worker.js`.
