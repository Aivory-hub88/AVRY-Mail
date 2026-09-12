# MCP Isolation and Capability Contract

> Planning document only. This is the security contract for user-level MCP, ZeroClaw Vanilla, and first-party Aivory Cerveau delegation. It is not an implementation specification yet and must be approved before code changes.

## 1. Security objective

An MCP request must be authorized for exactly one tenant and one mailbox. The caller may use only the tools and actions included in its server-issued capability. A model, prompt, tool argument, thread ID, attachment ID, or Cerveau payload cannot expand that authority.

The required invariant is:

```text
allowed(request) =
  valid_capability
  AND valid_audience
  AND valid_caller
  AND active_grant
  AND exactly_one_tenant
  AND exactly_one_mailbox
  AND requested_scope_is_granted
  AND object_belongs_to_execution_context
```

Any false condition produces a denial; it must not trigger a global or default fallback.

## 2. Trust boundaries

| Component | May do | Must not do |
|---|---|---|
| Aivory Mail API | Own data, policy, authorization, tool execution, confirmation, audit | Trust model-supplied identity or caller-provided mailbox scope |
| ZeroClaw Vanilla | Run Mail Assistant reasoning and call permitted tools | Read DB directly, retain broad secrets, switch mailbox |
| Aivory Cerveau | Orchestrate and call delegated Mail Assistant capability | Become mailbox owner, widen scopes, call DB, impersonate user |
| Dashboard | Obtain user consent, show grants, revoke grants | Render raw mailbox/provider secrets |
| External MCP client | Use OAuth-issued access token within granted scope | Use query-string keys or infer another mailbox |
| Mail content/attachments | Provide untrusted business data | Modify system instructions, identity, scopes, or policy |

## 3. Capability claims

The exact token format may be JWT or opaque reference token, but the authorization semantics are mandatory. A capability must represent:

```text
issuer       = AVRY-Mail authorization issuer
audience     = intended endpoint/runtime
caller       = agent or client identity
tenant_id    = one tenant
mailbox_id   = one mailbox
owner_type   = user | agent
agent_id     = required when owner_type=agent
scopes       = explicit allowlist
issued_at    = UTC timestamp
expires_at   = short expiry
jti          = unique token identifier
grant_id     = revocable server-side grant
confirmation = optional write binding
```

The server must validate signature/introspection, expiry, audience, caller, grant status, tenant, mailbox, and replay state before dispatching a tool.

### Recommended properties

- Short-lived access tokens for MCP runtime calls.
- Refresh/reissue controlled by the dashboard or first-party exchange, not by the model.
- `jti` or equivalent replay tracking for one-time exchanges and confirmations.
- Key rotation with an overlap window and an emergency revoke mechanism.
- No secrets in URLs, logs, tool results, or model-visible context.

## 4. ExecutionContext

Every protected request creates an immutable server-side context before business logic runs:

```text
ExecutionContext {
  tenant_id
  mailbox_id
  owner_type
  agent_id?
  caller_id
  audience
  scopes
  grant_id
  request_id
  trace_id
}
```

Handlers, repositories, memory providers, attachment stores, event publishers, and outbound senders receive this context or a narrower derived context. They must not accept an independent mailbox identity from the model.

If a legacy route still accepts `mailbox_id`, it may be used only as a consistency check against the resolved context. A mismatch is `403`; it must never cause context switching.

## 5. MCP protocol behavior

### Authentication

```http
POST /mcp
Authorization: Bearer <short-lived-mailbox-scoped-capability>
Content-Type: application/json
```

The MCP endpoint must reject:

- missing bearer capability;
- invalid signature/introspection;
- expired or revoked grant;
- wrong audience or caller;
- replayed exchange/one-time token;
- capability that contains no single mailbox;
- legacy global API key used as user-level authorization;
- `?api_key=` or other query-string credential.

### Tool schema rule

Tool arguments describe the operation, not authorization. This is allowed:

```json
{
  "name": "search_mail",
  "arguments": {
    "query": "invoice",
    "folder": "Inbox",
    "limit": 10
  }
}
```

This is not allowed as an authorization mechanism:

```json
{
  "name": "search_mail",
  "arguments": {
    "mailbox_id": "another-mailbox",
    "query": "invoice"
  }
}
```

If a compatibility schema temporarily contains `mailbox_id`, the server must ignore it for selection and reject mismatches during migration.

### Tool result rule

Results must be filtered and redacted by `ExecutionContext`. Error responses must not reveal whether another tenant/mailbox, message, thread, attachment, or capability exists.

## 6. Scope matrix

| Operation | Required scope | Default | Confirmation |
|---|---|---:|---|
| Search/list mail | `mail.search` or `mail.read` | deny | no |
| Read thread | `mail.thread.read` | deny | no |
| Read attachment | `mail.attachment.read` | deny | no |
| Read memory/knowledge | `mail.memory.read` / `mail.knowledge.read` | deny | no |
| Create draft | `mail.draft.create` | deny | optional policy |
| Send new mail | `mail.send` | deny | required by policy |
| Reply | `mail.reply` | deny | required |
| Move/archive | `mail.move` / `mail.archive` | deny | required for bulk |
| Delete | `mail.delete` | deny | required |
| Provision agent mailbox | `agent.mailbox.provision` | deny | mandatory human approval |
| Configure agent mailbox | `agent.mailbox.configure` | deny | mandatory for sensitive changes |
| Revoke/suspend | `agent.mailbox.revoke` | deny | dashboard/admin policy |

Scopes are additive only. A broad-looking label such as `mail.*` must not be accepted unless explicitly defined and intentionally granted by policy.

## 7. Mandatory data-plane predicates

Every read/write path must enforce the resolved context, including:

- messages and threads;
- drafts, sent items, labels, filters, signatures, and settings;
- attachments and object-store keys;
- thread memory and AI chat history;
- knowledge compiler input and cache keys;
- vector/FTS search and ranking candidates;
- realtime subscriptions and event fan-out;
- notifications and workflow/webhook payloads;
- outbound sender identity and aliases;
- audit records and usage counters.

The effective database rule is conceptually:

```sql
WHERE tenant_id = execution_context.tenant_id
  AND mailbox_id = execution_context.mailbox_id
```

A query that can return data before this predicate is applied is not acceptable for the isolated path.

## 8. Write confirmation contract

For send, reply, delete, bulk move, external forwarding, or other high-impact actions, AVRY-Mail must issue or validate a confirmation bound to:

```text
confirmation_id
mailbox_id
action
payload_hash
issued_at
expires_at
caller_id
one_time_use
```

The payload hash must cover recipient, sender, subject, body, attachments, and relevant action options. A modified payload, mailbox, caller, action, or expired confirmation is denied.

The model may request confirmation, but it cannot self-approve it.

## 9. Memory, cache, and prompt-injection defense

- Cache keys must include tenant, mailbox, and relevant scope/version.
- Conversation/session identifiers must not be globally reusable across mailboxes.
- Thread memory must verify that the thread belongs to the current context before loading.
- Attachments must be authorized both when referenced and when downloaded.
- Email body text, HTML, attachment text, and external content are untrusted input.
- Untrusted content cannot alter system instructions, tool permissions, recipient policy, or execution context.
- Model output is treated as a proposal until server-side validation succeeds.
- Logging must redact bodies, tokens, passwords, and sensitive attachment metadata.

## 10. First-party and external exchange

### Direct user / ZeroClaw

The dashboard or AVRY-Mail session authorizes the Mail Assistant. AVRY-Mail issues a capability scoped to the selected mailbox and approved scopes. ZeroClaw receives only the capability needed for the session/runtime.

### Aivory Cerveau

Cerveau requests a delegated capability from a first-party exchange. AVRY-Mail verifies the caller identity and user/tenant policy, then issues a capability with:

```text
caller = registered Cerveau agent
agent_id = delegated parent agent
mailbox_id = approved mailbox
scopes = explicitly approved subset
audience = Mail Assistant/MCP endpoint
short expiry + grant_id
```

Cerveau cannot exchange a mailbox ID it does not already have policy authority for, and cannot exchange its broad internal secret for user data access.

### External MCP client

Use OAuth 2.1 Authorization Code + PKCE. The consent screen shows tenant/mailbox, agent/client identity, scopes, expiry, and write implications. The OAuth access token maps to the same internal capability/execution-context model.

## 11. Failure and response policy

- `401`: no usable authentication, invalid token, expired token, or invalid token format.
- `403`: valid caller but wrong mailbox, tenant, audience, agent, scope, grant status, or confirmation binding.
- `404`: only for an object that is not visible in the current context; avoid existence leaks.
- `409`: replayed one-time exchange/confirmation or idempotency conflict.
- `429`: rate limit or abuse protection.

No failure path may retry with a global mailbox, default tenant, internal token, or unscoped query.

## 12. Isolation acceptance tests

Before enabling production MCP:

- [ ] User A cannot read User B messages, threads, memory, cache, notifications, or attachments.
- [ ] Agent A cannot read Agent B mailbox, even in the same tenant.
- [ ] Forged `mailbox_id` in any tool argument does not change context.
- [ ] Wrong tenant, caller, agent, audience, or grant is denied.
- [ ] Expired and revoked capabilities are denied.
- [ ] Replayed exchange and confirmation are denied.
- [ ] Search/vector/knowledge results cannot contain another mailbox candidate.
- [ ] Realtime and webhook events cannot cross mailbox boundaries.
- [ ] Prompt injection in an email cannot grant tools or alter scope.
- [ ] Attachment IDs from another mailbox are denied without existence leakage.
- [ ] Send/reply/delete confirmation fails after payload mutation.
- [ ] No token or mailbox secret appears in logs, URLs, model context, or tool results.
- [ ] Postgres and SQLite behavior enforce the same isolation invariant.

## 13. Legacy migration rule

The existing broad API-key, internal-token, Cerveau-header, and query-string-link paths are compatibility surfaces, not the target user-level authorization model. Each must receive an owner, scope, deprecation date, telemetry, and removal plan before the isolated connector is declared production-ready.
