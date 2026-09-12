# MCP External Patterns Adaptation Matrix

This document records the safe architectural patterns selected after reviewing [Wh1isper/mcp-email-server](https://github.com/Wh1isper/mcp-email-server) and [elie222/inbox-zero](https://github.com/elie222/inbox-zero). The patterns below are paraphrased design guidance; no external source code is copied into Aivory.

## License boundary

`mcp-email-server` reports BSD-3-Clause. `inbox-zero` is distributed under AGPLv3 with additional commercial-monetization and enterprise-use terms in its repository license. Aivory therefore reuses general engineering ideas only. Direct code reuse, derivative implementation, or redistribution requires a separate legal review and is intentionally out of scope.

## Adaptation matrix

| External pattern | Aivory target | Security/operational impact | Required test or gate | Decision |
|---|---|---|---|---|
| Immutable, centralized application limits | MCP v2 request parsing and tool argument validation | Prevents oversized queries, budgets, recipients, frames, and serialized responses from becoming provider or model-controlled resource exhaustion | Boundary tests for bytes/counts and rejection rather than silent widening | Adopt in MCP v2 |
| Static tool catalog with reviewed metadata | `mcp_v2_tools()` in `crates/aivory-mail-api/src/mcp.rs` | Keeps the exposed surface auditable; annotations help clients treat read-only and side-effecting tools differently without becoming authorization | Catalog snapshot/shape test; confirm no identity selectors or management tools | Adopt |
| Separate management/control plane from MCP data tools | `api/agent_access.rs` and confirmation issuance route | Prevents an MCP model from issuing/revoking its own capability or self-approving outbound mail | Existing admin auth tests plus no management tool in catalog | Already adopted |
| Ports/adapters and bounded provider calls | Existing `ExecutionContext`, knowledge compiler, and outbound wrapper | Keeps authorization in the API boundary and avoids trusting provider/model identity; remaining legacy paths stay explicitly out of v2 readiness | Context-isolation tests and provider parity checks | Adopt for v2; do not refactor all legacy paths now |
| Typed mutation outcomes including unknown/reconciliation-needed | v2 send result mapping and outbound boundary | Avoids claiming failure after an external provider may have accepted the message; prevents unsafe automatic retry/duplicate delivery | Deterministic confirmation tests cover expiry, binding, and concurrent replay. `sqlite_v2_send_reports_reconciliation_when_persistence_fails_after_dispatch` covers a provider-accepted, post-effect persistence failure | Adopted for MCP v2 send; legacy `/mcp` send_mail also stopped leaking raw error text to the caller |
| Redacted operational logging (no addresses, subjects, bodies, or provider response text in logs) | Outbound transport logging in `mail/outbound.rs` | Prevents email content and provider response bodies from persisting in log storage/observability pipelines | Manual review of log call sites at each transport | Adopted: transport success/attempt logs now carry only counts and provider name, not from/to/subject/body |
| Draft-first and explicit human confirmation | `mcp_confirmations.rs` | Binds action, caller, tenant, mailbox, payload, expiry, and one-time use; the model cannot self-approve | Payload mutation, expiry, wrong caller/mailbox, and replay tests | Already adopted |
| Prompt hardening for untrusted email/attachment content | Cerveau/assistant orchestration layer, not the Rust MCP handler | Prevents message text from changing system instructions or requesting unauthorized tools | Assistant regression/eval suite before enabling autonomous writes | Document as external dependency; not implemented in this phase |
| Delayed actions with durable status/history | Future Aivory scheduler/action ledger | Makes deferred side effects observable and recoverable; must remain context-bound | Durable state-machine, retry/idempotency, and audit tests | Backlog; not a prerequisite for read-only MCP v2 |
| Broad AI regression/evaluation harness | Aivory assistant test/eval package | Detects tool availability drift, partial failures, memory-write inference, and send-disabled regressions | Add after assistant tool surface is connected to Aivory MCP | Backlog; deterministic MCP tests are the current gate |
| Safe attachment policy and no arbitrary file reader | Future attachment-scoped capability path | Prevents path traversal, cross-mailbox reads, and uncontrolled result expansion | Separate attachment authorization, size, MIME, and storage tests | Do not expose attachment tools in current v2 |

## Phase 2 implementation scope

The implementation in this phase is limited to the rows marked **Adopt** or **Already adopted**. It does not enable production MCP, replace legacy MCP behavior, or claim that realtime, attachment, event, cache, and all outbound paths are uniformly `ExecutionContext`-aware. Those remain explicit acceptance blockers.

Content was rephrased for compliance with licensing restrictions; consult the linked repositories for the original implementations and license terms.
