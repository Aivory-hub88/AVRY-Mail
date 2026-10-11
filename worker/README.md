# Aivory Mail Cloudflare Adapter

This directory holds the Cloudflare Worker shim that forwards Email Routing events to the Aivory Mail API.

- `email()` handler → `POST /v1/webhooks/cloudflare` on your Aivory Mail deployment
- `fetch()` handler → proxies dashboard/API (optional)

## Token (required)

The worker authenticates to the API with the server's `INTERNAL_TOKEN`, sent
as `x-internal-token`. Store it as a Worker **secret** — wrangler does not
expand `${...}` in `wrangler.jsonc`, so a value written there is sent literally
and is rejected by the API:

```bash
cd worker
echo "<the INTERNAL_TOKEN from the server's .env>" | npx wrangler secret put AIVORY_MAIL_API_TOKEN
npx wrangler secret list   # AIVORY_MAIL_API_TOKEN must be listed
```

Mail the API rejects makes the worker throw, so Cloudflare retries and then
bounces it to the sender; it is never silently dropped.

For full Rust Worker (wasm), build `crates/aivory-mail-api` with `worker` feature (requires `worker` crate).
For MVP, use the JS shim in `worker.js`.
