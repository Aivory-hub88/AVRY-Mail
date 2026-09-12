# Phase 2 — MCP Mailbox Capability Design

> This design defines the first mailbox-scoped MCP capability implementation. It does not enable production MCP, agent mailbox provisioning, OAuth, or Cerveau delegation by itself.

## 1. Security objective

MCP must authorize one tenant, one mailbox, one caller, one audience, and an explicit scope set before parsing or executing a data tool. The model cannot provide or alter mailbox identity.

```text
Bearer capability → hash lookup → active grant → tenant/mailbox context
                   → caller/audience check → scope check → tool execution
```

Missing, malformed, expired, revoked, wrong-audience, wrong-caller, or scope-insufficient capabilities fail closed.

## 2. Token choice

Use an opaque, cryptographically random bearer token rather than a self-contained JWT for the initial MCP grant:

- raw token is returned only at issuance;
- only SHA-256 hash is persisted;
- revocation is immediate through the grant row;
- changing grant state does not require token signing-key rotation;
- token claims are never model-visible or user-editable;
- token is sent only in `Authorization: Bearer`, never in a URL.

The token is short-lived. Initial policy:

- default lifetime: 15 minutes;
- maximum lifetime: 1 hour;
- no indefinite MCP grant;
- refresh/reissue is a control-plane operation, not a tool call.

Bearer replay cannot be solved by a database lookup alone. Transport must be TLS, tokens must be short-lived, grants must be revocable, and one-time exchange/confirmation replay protections are separate later controls.

## 3. Persistent grant model

Add forward migration `025_mcp_capability_grants.sql` for Postgres and equivalent SQLite `ensure_schema` creation.

```text
mcp_capability_grants
├── id UUID primary key                  # grant_id
├── tenant_id UUID/TEXT not null
├── mailbox_id UUID/TEXT not null
├── caller_id TEXT not null              # registered agent/client identity
├── audience TEXT not null               # e.g. aivory-mail-mcp
├── scopes TEXT/JSON not null             # explicit JSON array
├── token_hash TEXT unique not null
├── jti UUID/TEXT unique not null
├── expires_at timestamp/text not null
├── revoked_at timestamp/text nullable
├── last_used_at timestamp/text nullable
└── created_at timestamp/text not null
```

The grant row must validate that `mailbox_id` belongs to `tenant_id` at issuance. A mismatched pair is rejected; it must not be repaired by selecting a different tenant.

Indexes:

- unique `token_hash`;
- unique `jti`;
- `(tenant_id, mailbox_id, revoked_at)`;
- `(expires_at, revoked_at)`.

No raw token, password, IMAP/SMTP secret, or internal service secret is stored.

## 4. Execution context from capability

A valid grant resolves to:

```text
ExecutionContext {
  tenant_id,
  mailbox_id,
  owner_type = Agent,
  caller_id,
  audience,
  scopes,
  grant_id,
  request_id,
  trace_id,
}
```

`mailbox_id` is never read from MCP tool arguments. Any compatibility argument supplied by a client is ignored for selection; a future strict mode may reject it as an invalid schema.

## 5. Scope catalog for MCP

| Tool | Required scope |
|---|---|
| `search_mail` | `mail.search` |
| `get_inbox_overview` | `mail.read` |
| `get_thread_memory` | `mail.thread.read` |
| `get_knowledge_compile` | `mail.knowledge.read` |
| `send_mail` | `mail.send` plus later confirmation |

The following are not enabled by this phase:

- delete;
- bulk move/archive;
- forwarding;
- mailbox provisioning;
- grant management from MCP;
- arbitrary webhook/event dispatch.

Scope checks happen before database queries. A denied scope must not reveal object existence.

## 6. Capability lifecycle control plane

Initial lifecycle is admin-only and server-side:

```text
POST   /v1/agent-access/grants       issue grant; return raw token once
GET    /v1/agent-access/grants       list metadata, never raw token
DELETE /v1/agent-access/grants/:id   revoke grant immediately
```

Issuance request must include:

```json
{
  "tenant_id": "...",
  "mailbox_id": "...",
  "caller_id": "zeroclaw-mail-assistant",
  "audience": "aivory-mail-mcp",
  "scopes": ["mail.search", "mail.read"],
  "expires_in_seconds": 900
}
```

The route must validate:

- admin control-plane authorization;
- tenant/mailbox ownership pair;
- non-empty caller and audience;
- scopes against an allowlist;
- expiry within configured bounds;
- no raw token in logs or subsequent list responses.

This is not the final user dashboard flow. User consent, registered agent identity, OAuth, and Cerveau exchange remain later phases.

## 7. Authentication rejection rules

MCP must reject:

- `x-internal-token` as user mailbox authorization;
- `x-cerveau-internal-secret` as user mailbox authorization;
- query-string API keys;
- legacy global API keys;
- missing bearer token;
- bearer token whose hash is absent;
- revoked grant;
- expired grant;
- wrong audience;
- wrong trusted caller binding;
- malformed scopes;
- grant whose tenant/mailbox pair no longer exists.

The existing `AVRY_MCP_CAPABILITY_MODE` gate remains disabled by default. It may be set to `v2` only after the capability and isolation tests pass; implementation work must not automatically set it in deployment files.

## 8. MCP schema contract

Tool schemas must contain only operation parameters:

```json
{
  "name": "search_mail",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {"type": "string"},
      "folder": {"type": "string"},
      "limit": {"type": "integer"}
    },
    "required": ["query"]
  }
}
```

No MCP tool schema may expose or require:

- `mailbox_id`;
- `tenant_id`;
- an alternate owner identity;
- an API key;
- a database/storage key.

## 9. Error policy

- `401`: missing, malformed, unknown, expired, or revoked bearer capability.
- `403`: wrong audience/caller, insufficient scope, or context mismatch.
- `404`: only for an object not visible within the resolved context.
- `409`: replayed one-time exchange or conflicting lifecycle operation.
- `429`: rate limit.

MCP JSON-RPC errors should map these HTTP conditions without returning mailbox existence or SQL details.

## 10. SQLite and Postgres parity

- Postgres migration uses UUID, `TIMESTAMPTZ`, and JSON/JSONB-compatible scope text.
- SQLite bootstrap uses TEXT UUIDs, RFC3339 timestamps, and JSON text.
- Token expiry is parsed and checked in Rust, not by backend-specific SQL.
- Issuance and revocation have explicit `$1` and `?` statements.
- Existing SQLx migration files remain unchanged; `025` is append-only.
- `ensure_schema` creation is idempotent and must not silently downgrade an existing grant table.

## 11. Phase 2 acceptance gate

- [ ] Raw token is returned once and never persisted/logged in plaintext.
- [ ] Grant always binds one valid tenant/mailbox pair.
- [ ] MCP can resolve context without tool-supplied mailbox ID.
- [ ] Wrong audience/caller/tenant/mailbox is denied.
- [ ] Expired and revoked grants are denied.
- [ ] Scope denial happens before any data query.
- [ ] API-key, query-key, and internal-secret paths cannot access MCP data.
- [ ] MCP schemas contain no mailbox/tenant identity arguments.
- [ ] Search, overview, memory, knowledge, and send use the resolved context.
- [ ] Postgres and SQLite grant behavior is equivalent.
- [ ] Cross-mailbox, replay/expiry/revoke, and prompt-forged identity tests pass.
- [ ] `AVRY_MCP_CAPABILITY_MODE` remains disabled until canary approval.

## 12. Deferred security work

- OAuth 2.1 + PKCE for external MCP clients.
- First-party Cerveau delegated exchange with cryptographic caller identity.
- Registered agent catalog and agent mailbox ownership.
- Confirmation tokens and payload hashes for send/reply/delete/bulk operations.
- Rate limiting, anomaly detection, and emergency global revocation.
- Dashboard consent/revoke UI.
