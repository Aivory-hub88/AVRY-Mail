# Aivory Mail — API Reference

Base URL: `http://localhost:8095` (dev) / `https://mail.aivory.uk` (prod).

Envelope: `{ "success": bool, "data": … }`, errors `{ "success": false, "error": … }`.

## Health & Stats

| Method | Path          | Description                          |
|--------|---------------|--------------------------------------|
| GET    | `/health`     | Health check (`status/version/mode/storage/db`) |
| GET    | `/v1/health`  | Alias of `/health`                   |
| GET    | `/v1/stats`   | Counts: domains, mailboxes, messages |

## Domains

| Method | Path                    | Description                          |
|--------|-------------------------|--------------------------------------|
| GET    | `/v1/domains`           | List domains                         |
| POST   | `/v1/domains`           | Create domain `{domain}`             |
| GET    | `/v1/domains/:id`       | Domain detail                        |
| DELETE | `/v1/domains/:id`       | Delete domain                        |
| POST   | `/v1/domains/:id/verify`| Mark verified                        |
| GET    | `/v1/domains/:id/dns`   | DNS status (Cloudflare)              |

## Mailboxes

| Method | Path                | Description                                    |
|--------|---------------------|------------------------------------------------|
| GET    | `/v1/mailboxes`     | List (`?domain_id=`)                           |
| POST   | `/v1/mailboxes`     | Create `{address, display_name, is_catch_all, forward_to}` |
| GET    | `/v1/mailboxes/:id` | Detail                                         |
| PUT    | `/v1/mailboxes/:id` | Update `{display_name}`                       |
| DELETE | `/v1/mailboxes/:id` | Delete                                         |

## Messages & Threads

| Method | Path                                            | Description                         |
|--------|-------------------------------------------------|-------------------------------------|
| GET    | `/v1/messages`                                   | List. Query: `folder`(default Inbox), `mailbox_id`, `search`, `page`, `per_page` (≤100) |
| GET    | `/v1/messages/:id`                               | Get one (marks read)                |
| DELETE | `/v1/messages/:id`                               | Delete                             |
| PUT    | `/v1/messages/:id/read`                          | `{is_read: bool}`                   |
| POST   | `/v1/messages/:id/move`                          | `{folder}` — allowed: Inbox, Sent, Drafts, Spam, Trash, Archive |
| GET    | `/v1/messages/:id/attachments/:att_id`           | Download attachment (binary)        |
| POST   | `/v1/messages/:id/star`                          | Toggle star                         |
| POST   | `/v1/messages/:id/share`                         | Create share link (7d JWT)          |
| GET    | `/v1/threads`                                    | List threads                        |
| GET    | `/v1/threads/:id`                                | Thread + messages                   |
| POST   | `/v1/threads/:id/reply`                          | Reply to thread                     |
| GET    | `/v1/threads/:id/crawl`                          | Crawl thread context                |
| GET/POST | `/v1/threads/:id/follow-up`                    | Follow-up suggestion / trigger      |
| GET    | `/v1/threads/:id/memory`                         | Budgeted thread memory for LLM      |
| GET    | `/v1/drafts`                                     | List drafts (folder=Drafts)         |
| POST   | `/v1/drafts`                                     | Save/update draft                   |

## Shared messages

| Method | Path           | Description                                 |
|--------|----------------|---------------------------------------------|
| GET    | `/v1/share/:id`| Get shared message. Query `?t=<token>` (JWT) |

## Send

| Method | Path              | Description                                      |
|--------|-------------------|--------------------------------------------------|
| POST   | `/v1/send`        | Send `{from, to[], cc[], bcc[], subject, text/html, attachments}` → `{id, status:"queued"}` |
| POST   | `/v1/send/batch`  | Batch ≤50 messages `{messages: [...]}`           |

## Intelligence & AI

| Method | Path                      | Description                                   |
|--------|---------------------------|-----------------------------------------------|
| POST   | `/v1/intelligence/analyze`| Heuristic + optional AI gateway analyze       |
| POST   | `/v1/intelligence/suggest`| Suggest actions/replies                       |
| POST   | `/v1/agent/actions`       | Dispatch agent action                         |
| GET    | `/v1/knowledge/compile`   | Auto-compile agent knowledge per tenant/scope |

## MCP (Remote)

| Method | Path              | Description                                     |
|--------|-------------------|--------------------------------------------------|
| GET    | `/v1/mcp/tools`   | List MCP tools                                  |
| GET    | `/mcp`            | MCP tools (GET)                                 |
| POST   | `/mcp`            | MCP JSON-RPC: `initialize`, `tools/list`, `tools/call` |
| POST   | `/v1/mcp/generate-link` | Generate MCP connection URL `{name | key_id}` |

Tools exposed by MCP: `search_mail`, `get_inbox_overview`, `get_thread_memory`,
`get_knowledge_compile`, `send_mail`.

## Search

| Method | Path                 | Description                           |
|--------|----------------------|---------------------------------------|
| GET    | `/v1/search`         | Hybrid search (vector+FTS) `?q=`      |
| GET    | `/v1/inbox/overview` | 1-call inbox stats                    |
## Calendar

| Method | Path                          | Description                              |
|--------|-------------------------------|------------------------------------------|
| GET    | `/v1/calendar/status`         | Calendar status (linked providers)       |
| GET    | `/v1/calendar/event-types`    | Booking event types                      |
| GET    | `/v1/calendar/slots`          | Available slots                          |
| POST   | `/v1/calendar/bookings`       | Create booking                           |
| POST   | `/v1/calendar/propose`        | Propose slots for a thread/message       |
| GET    | `/v1/calendar/events`         | List events (Google-parity CRUD)         |
| POST   | `/v1/calendar/events`         | Create event `{title, start_at, end_at, …}` |
| PUT    | `/v1/calendar/events/:id`     | Update `{title, start_at, end_at, calendar}` |
| DELETE | `/v1/calendar/events/:id`     | Delete event                             |

## Webhooks (inbound)

| Method | Path                      | Description                                    |
|--------|---------------------------|------------------------------------------------|
| POST   | `/v1/webhooks/inbound`    | Generic inbound (JSON or raw MIME base64)      |
| POST   | `/v1/webhooks/cloudflare` | Cloudflare Email Routing worker payload        |

## Signatures

| Method | Path              | Description                                    |
|--------|-------------------|------------------------------------------------|
| GET    | `/v1/signatures`  | List (`?mailbox_id=`)                          |
| POST   | `/v1/signatures`  | Create `{mailbox_id, name, html, text, is_default}` |
| PUT    | `/v1/signatures/:id` | Update `{name, html, text, is_default}`     |
| DELETE | `/v1/signatures/:id` | Delete                                      |

## User settings (Gmail/Zoho/Outlook parity)

| Method | Path                 | Description                                    |
|--------|----------------------|------------------------------------------------|
| GET    | `/v1/settings`       | Get settings `?category=` (seeded with defaults) |
| POST   | `/v1/settings`       | Set `{category, key, value, mailbox_id?}`       |
| GET    | `/v1/labels`         | List labels                                    |
| POST   | `/v1/labels`         | Create label `{name, color}`                   |
| DELETE | `/v1/labels/:id`     | Delete label                                   |
| GET    | `/v1/filters`        | List filters/rules                             |
| POST   | `/v1/filters`        | Create filter `{name, criteria, action}`       |
| GET    | `/v1/vacation`       | Get vacation responder `?mailbox_id=`          |
| POST   | `/v1/vacation`       | Set vacation responder `{mailbox_id, enabled, subject, body}` |

> Categories & defaults: see [USER_SETTINGS.md](./USER_SETTINGS.md).

## API Keys

| Method | Path                  | Description                                        |
|--------|-----------------------|----------------------------------------------------|
| GET    | `/v1/api-keys`        | List (auto-creates a default `avry-…` dev key)     |
| POST   | `/v1/api-keys`        | Create `{name}` → returns `key_raw` **once**       |
| DELETE | `/v1/api-keys/:id`    | Delete key                                         |

Keys are SHA-256 hashed for storage; the raw value is returned only at
creation and kept in `key_raw` so the UI can render a consistent masked +
reveal (`avry-…`).

## Realtime

| Method | Path                  | Description                          |
|--------|-----------------------|--------------------------------------|
| GET    | `/v1/realtime/ws`     | WebSocket (`?mailbox_id=`) push for new mail/updates |

## Cognee / Knowledge

| Method | Path              | Description                                  |
|--------|-------------------|----------------------------------------------|
| GET    | `/v1/cognee/sync` | Sync mailbox ingestion to cognee sidecar     |