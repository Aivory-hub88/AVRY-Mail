# Aivory Mail Cloudflare Adapter

This directory holds the Cloudflare Worker shim that forwards Email Routing events to the Aivory Mail API.

- `email()` handler → `POST /v1/webhooks/cloudflare` on your Aivory Mail deployment
- `fetch()` handler → proxies dashboard/API (optional)

For full Rust Worker (wasm), build `crates/aivory-mail-api` with `worker` feature (requires `worker` crate).
For MVP, use the JS shim in `worker.js`.
