# Phase 1 — Authorization Foundation Design

> This design is the implementation boundary for the first execution phase. It deliberately does not enable agent mailbox provisioning, delegated Cerveau grants, or production MCP data access.

## Objective

Create one server-resolved mailbox context for authenticated first-party user requests and make the unsafe legacy MCP service boundary fail closed until Phase 2 capability work is complete.

## Phase 1 scope

### In scope

- Canonical `ExecutionContext` type for one request.
- JWT identity resolution with server-side mailbox and tenant lookup.
- Tenant claim issuance for mailbox login where the mailbox has a tenant.
- Default-deny behavior when an isolated route cannot resolve exactly one mailbox.
- Explicit separation between user data-plane context and admin control-plane routes.
- MCP safety gate that does not treat internal service secrets or global API keys as mailbox authorization.
- Unit tests for context resolution and default-deny cases.

### Out of scope

- Persistent agent capability/grant tables.
- OAuth 2.1 + PKCE.
- Cerveau delegated exchange.
- Agent mailbox provisioning.
- Write confirmation tokens.
- Full migration of every legacy handler to context-aware repositories.
- Production re-enablement of MCP data tools.

## Context model

```text
ExecutionContext {
  tenant_id: UUID,
  mailbox_id: UUID,
  principal: String,
  owner_type: User,
  caller_id: String,
  audience: String,
  scopes: Set<Scope>,
  grant_id: Option<UUID>,
  request_id: String,
  trace_id: Option<String>,
}
```

Phase 1 only constructs the user-session form. `grant_id` remains absent until the Phase 2 capability model exists. The context is immutable after resolution and must be passed down or narrowed; handlers may not replace `tenant_id` or `mailbox_id` from request/model fields.

## Resolution rules

1. Verify the bearer JWT signature and expiry.
2. Normalize the authenticated principal from `sub`.
3. Resolve one mailbox by normalized address.
4. Resolve its tenant from the mailbox row.
5. If no mailbox exists, deny the isolated operation.
6. If more than one mailbox matches, deny rather than choose an arbitrary row.
7. If a request includes a mailbox selector, it is a consistency check only; mismatch is `403`.
8. An admin identity without an explicit mailbox context is not a valid data-plane context. Admin-wide behavior remains in the separately gated control plane.
9. Never use `default`, nil UUID, `None`, or a global query as an isolated fallback.

## JWT compatibility

The shared verifier already accepts optional `tenant_id` and `role` claims. Login must add the mailbox tenant ID when issuing a mailbox session. Existing tokens without a tenant claim remain parseable for compatibility but cannot satisfy a future route that requires a tenant-bound context; users must re-authenticate when the isolated route is enabled.

## MCP safety behavior

Until Phase 2 supplies a short-lived mailbox-scoped capability:

- `x-internal-token`, `x-cerveau-internal-secret`, query API keys, and unscoped API keys are not accepted as user mailbox authorization.
- MCP data calls must fail closed rather than parse `mailbox_id` from model arguments.
- No global health fallback may access mail data.
- Re-enablement requires the Phase 2 capability implementation and isolation tests.

This is intentionally a temporary compatibility break. It is safer than leaving the current service-wide trust boundary active while the new contract is incomplete.

## Files and responsibilities

| Area | Phase 1 responsibility |
|---|---|
| `api/execution_context.rs` | Context type, scope enum, context resolution helpers |
| `api/authz.rs` | Canonical authenticated claims/principal and mailbox/tenant resolver |
| `api/auth.rs` | Include tenant claim in newly issued mailbox JWTs |
| `api/mod.rs` | Register module and preserve admin/data-plane route separation |
| `mcp.rs` | Enforce the temporary fail-closed capability gate |
| tests | Context resolution/default-deny and MCP legacy-auth denial |
| migrations | None required for the user-session foundation; persistent grants are Phase 2 |

## Database compatibility

The resolver must use two explicit query forms:

- Postgres: UUID bind parameters and typed UUID extraction.
- SQLite: string bind parameters and UUID parsing at the boundary.

No `ensure_schema` change is required for the user-session context. Phase 2 grant persistence must add a forward migration and an equivalent SQLite schema path; it must not rewrite applied SQLx migration history.

## Acceptance criteria

- [ ] A user request resolves to exactly one tenant and mailbox or is denied.
- [ ] The resolved mailbox cannot be replaced by a request or model argument.
- [ ] A token without a resolvable mailbox cannot access an isolated data path.
- [ ] Admin-wide routes remain explicit control-plane routes.
- [ ] Legacy MCP service credentials cannot read or write mailbox data.
- [ ] MCP does not use global or default mailbox fallback.
- [ ] Newly issued mailbox JWTs contain the resolved tenant ID.
- [ ] Existing source behavior outside the Phase 1 boundary is not silently claimed to be isolated.
- [ ] `cargo fmt --check`, targeted tests, and `cargo check -p aivory-mail-api` pass.

## Phase gate

Phase 1 is not a production connector release. It is complete only when the foundation tests pass and the remaining legacy data paths are explicitly tracked for Phase 2 migration. MCP data access remains disabled until the Phase 2 capability gate is approved.
