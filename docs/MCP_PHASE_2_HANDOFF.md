# MCP Phase 2 Handoff

Status handoff: **implemented locally, production MCP remains disabled**.

This document is the operational handoff for the Phase 2 MCP capability/isolation work in `AVRY-Mail`. It records what is implemented, what was validated, why Postgres parity is still pending, and the acceptance gates that must be completed before enabling MCP v2 in production.

## 1. Executive decision

Do **not** enable MCP v2 in production yet.

The Phase 2 capability, confirmation, scoped-tool, and deterministic isolation behavior is implemented and passes the available local validation. Production readiness is still blocked by:

- Postgres parity not being exercised because `TEST_DATABASE_URL` was not set;
- a staging test for provider failure after an outbound side effect, including reconciliation behavior;
- incomplete `ExecutionContext` enforcement across legacy/realtime/attachment/event/cache and all outbound data-plane paths;
- a staging/canary run with isolated resources and synthetic data;
- prompt-hardening and broader assistant regression/evaluation coverage for autonomous use.

The production feature gate must remain unset/disabled:

```text
AVRY_MCP_CAPABILITY_MODE=v2
```

`v2` is the opt-in staging/test value. This handoff does not authorize setting it in production, changing deployment configuration, changing secrets, committing, pushing, or deploying.

## 2. Implemented Phase 2 behavior

### Capability and execution context

- Opaque bearer grants are issued by the control plane; only SHA-256 token hashes are persisted.
- Grants bind one `tenant_id`, one `mailbox_id`, one `caller_id`, one `audience`, an explicit scope set, expiry, and revocation state.
- Capability resolution creates an immutable `ExecutionContext` before protected MCP tool execution.
- Expiry, revoke, audience, caller, tenant/mailbox relationship, and scope checks fail closed.
- User/model-supplied identity is not an authority source. MCP v2 tool arguments do not select a mailbox or tenant.
- Wrong valid bindings are treated as forbidden rather than switching context or falling back to a global mailbox.

### Send confirmation

- High-impact send execution requires a control-plane-issued confirmation.
- The confirmation is bound to the action, payload hash, caller, tenant, mailbox, and expiry.
- Payload hashing covers the send request so recipient/body/options mutation cannot reuse a confirmation.
- Confirmation consumption is atomic and one-time through `used_at`, preventing concurrent replay.
- Confirmation is consumed before outbound side effects to avoid retry-driven duplicate delivery.
- If an error occurs after confirmation and the provider outcome is ambiguous, MCP returns the conservative unknown/reconciliation-required result instead of claiming definite failure or automatically retrying.

### MCP v2 tools and protocol surface

The reviewed v2 catalog currently covers scoped:

- `search_mail`;
- `get_inbox_overview`;
- `get_thread_memory`;
- `get_knowledge_compile`;
- `send_mail`.

The v2 surface also includes:

- stricter JSON-RPC request/argument validation;
- top-level errors for unknown tools, including nested/invalid dispatch paths;
- static reviewed catalog annotations for read-only, destructive, idempotency, and open-world behavior;
- centralized immutable request, result, thread, and send limits;
- rejection of malformed, oversized, or out-of-range arguments rather than silent clamping;
- bounded thread reads before body/resource work is performed;
- no capability-management or mailbox-selection tool in the v2 catalog.

Legacy MCP behavior remains available when v2 is disabled. Phase 2 did not replace the legacy compatibility surface globally.

## 3. Main implementation files

The relevant implementation and test files are:

- `crates/aivory-mail-api/src/mcp.rs` — MCP protocol handling, v2 dispatch, schemas, catalog, and errors.
- `crates/aivory-mail-api/src/mcp_limits.rs` — centralized MCP request/result/send bounds.
- `crates/aivory-mail-api/src/api/mcp_capabilities.rs` — capability grant issuance, lookup, expiry, and revoke behavior.
- `crates/aivory-mail-api/src/api/mcp_confirmations.rs` — send confirmation issuance and atomic one-time consumption.
- `crates/aivory-mail-api/src/api/execution_context.rs` — resolved tenant/mailbox/caller execution context.
- `crates/aivory-mail-api/src/api/knowledge.rs` — context-aware knowledge path used by v2.
- `crates/aivory-mail-api/src/mail/outbound.rs` — outbound boundary and conservative unknown-outcome mapping.
- `crates/aivory-mail-api/tests/phase2_mcp_isolation.rs` — capability, isolation, confirmation, JSON-RPC, scope, and lifecycle integration coverage.
- `migrations/025_mcp_capability_grants.sql` — Postgres capability grant schema.
- `migrations/029_mcp_send_confirmations.sql` — Postgres send confirmation schema.
- `docs/MCP_ISOLATION_CAPABILITY_CONTRACT.md` — isolation contract and mandatory predicates.
- `docs/PHASE_2_MCP_CAPABILITY_DESIGN.md` — original Phase 2 capability design.
- `docs/MCP_EXTERNAL_PATTERNS_ADAPTATION_MATRIX.md` — external pattern research and adaptation decisions.
- `docs/STAGING_MCP_E2E.md` — isolated staging live E2E runbook.

## 4. Validation completed

The following validation was completed in the AVRY-Mail repository root:

```bash
cargo check -p aivory-mail-api
```

Result: **passed**.

```bash
cargo check --workspace
```

Result: **passed**.

```bash
cargo test -p aivory-mail-api --test phase2_mcp_isolation -- --nocapture
```

Result: **10 passed**. The Postgres parity portion reported the explicit skip described in the next section because `TEST_DATABASE_URL` was not set.

```bash
cargo test -p aivory-mail-api --lib mcp_ -- --nocapture
```

Result: **9 passed**, including the static, scoped, annotated v2 catalog coverage (`v2_catalog_is_static_scoped_and_annotated`).

Additional checks:

```bash
rustfmt --edition 2021 --check crates/aivory-mail-api/src/mcp_limits.rs
```

Result: **passed** for the newly added limits module.

```bash
git diff --check
```

Result: **passed** for the current diff at the time of validation.

`cargo fmt --all -- --check` was not used as a Phase 2 acceptance signal because the checkout contains many pre-existing/unrelated formatting changes. Do not run broad formatting as part of this handoff; it can rewrite unrelated work.

Existing compiler warnings are non-blocking and include deprecated `mail_auth::common::crypto::RsaKey::<T>::from_rsa_pem`, unused variables, and dead code. No compilation errors were observed.

## 5. Why Postgres parity is not yet validated

The parity test is intentionally opt-in because it mutates the configured database and deletes generated rows. The validation shell did not contain `TEST_DATABASE_URL`, so the test emitted:

```text
skipping postgres parity test: TEST_DATABASE_URL is not set
```

This is a **missing test environment**, not a reported Postgres failure. SQLite/local behavior was exercised by the available deterministic tests, but that does not prove that the Postgres migrations, parameter binding, timestamp behavior, JSON scope storage, indexes, and cleanup behave equivalently against a real database.

Do not point the test at production, a shared development database, or any database containing user mail. A future engineer must provide a disposable, isolated Postgres database with the current migrations applied.

### Safe rerun

From `/Users/ireichmann/Documents/Aivory/AVRY-Mail-audit`, set a non-production database URL only in the current shell:

```bash
export TEST_DATABASE_URL='postgres://USER:PASSWORD@HOST:5432/TEST_DATABASE'
cargo test -p aivory-mail-api --test phase2_mcp_isolation -- --nocapture
unset TEST_DATABASE_URL
```

Use a database dedicated to this test. Confirm that it is not production and that the credentials are not written to the repository, `.env` files, logs, or test output. The test is expected to create and remove its generated records; verify the database backup/cleanup policy before running it.

A successful rerun must show the normal Phase 2 integration assertions and a completed Postgres parity result rather than the skip message. Record the exact command output and database/migration revision in the next handoff, without recording credentials.

## 6. Remaining blockers and acceptance gates

### Required before a production canary

1. **Postgres parity:** run the opt-in parity test against a disposable database with current migrations and record a passing result.
2. **Post-effect provider failure test:** inject a failure after the provider may have accepted a message. Verify that the API reports an unknown/reconciliation-required outcome, does not automatically retry, and does not allow an unsafe duplicate send path.
3. **Isolated staging E2E:** follow `docs/STAGING_MCP_E2E.md` with separate API, Postgres, storage, network, credentials, synthetic tenants, synthetic mailboxes, and synthetic message sentinels. Keep `AVRY_MCP_CAPABILITY_MODE=v2` limited to staging.
4. **Migration and deployment preflight:** verify the deployed migration ledger, health, base configuration, and rollback path without changing production configuration permanently.
5. **Grant cleanup:** verify that staging/canary grants are revoked or deleted and that no raw bearer tokens appear in logs or artifacts.
6. **Read-only canary first:** if approval is granted, exercise only mailbox-scoped read/list/search calls before considering any write capability.

### Broader isolation work still required

The current v2 work does not claim that every Aivory data-plane path is uniformly context-aware. Before enabling broader autonomous MCP behavior, review and test:

- realtime WebSocket subscriptions and event fan-out;
- attachment references, downloads, object-store keys, and MIME/size limits;
- legacy knowledge paths outside the v2 context-aware route;
- cache keys, thread memory, notifications, workflow/webhook payloads, and audit/usage paths;
- every outbound sender identity, alias, persistence, and provider adapter path;
- any remaining route that accepts `mailbox_id` independently of the resolved context.

A mismatch must deny access; it must never switch context or fall back to a default tenant/mailbox.

### Assistant-layer follow-up

The following were deliberately not pulled into this Phase 2 code change:

- prompt hardening for hostile/untrusted email and attachment content;
- delayed-action scheduler and durable action history;
- attachment MCP capabilities;
- OAuth 2.1 + PKCE and first-party Cerveau delegated exchange;
- broad AI regression/evaluation harness.

These are separate readiness gates for an AI assistant that can autonomously interpret mailbox content or perform writes. The MCP handler must continue treating email bodies, HTML, attachments, and external content as untrusted data.

## 7. External research boundary

The implementation adapted general safety patterns from the reviewed projects without copying external source code:

- `Wh1isper/mcp-email-server` — BSD-3-Clause; relevant patterns included centralized bounds, ports/adapters, bounded provider operations/results, static catalog review, and explicit unknown mutation outcomes.
- `elie222/inbox-zero` — AGPLv3 with additional commercial/enterprise terms; relevant patterns included declarative/allowlisted tool surfaces, confirmation or draft-first flows, prompt hardening, durable action state, and evaluation coverage.

Direct source reuse was intentionally avoided. The adaptation decisions and license boundary are documented in `docs/MCP_EXTERNAL_PATTERNS_ADAPTATION_MATRIX.md`.

## 8. Git and deployment state

- No commit was created for this handoff.
- No push was performed.
- No deployment was performed.
- No production secret or production configuration was changed.
- The repository already contains unrelated dirty/untracked files; preserve them.
- Do not reset, clean, broadly stage, or format the repository while continuing this work.

## 9. Recommended next sequence

1. Provision or identify a disposable non-production Postgres database and run the safe parity command above.
2. Record the parity result and migration revision without exposing credentials.
3. Add/run the provider post-effect failure and reconciliation acceptance test in staging.
4. Execute the isolated staging read-only E2E from `docs/STAGING_MCP_E2E.md`.
5. Audit the remaining data-plane paths for `ExecutionContext` propagation and add denial tests for each boundary.
6. Obtain explicit canary approval before setting `AVRY_MCP_CAPABILITY_MODE=v2` anywhere outside the isolated staging environment.
7. Keep production disabled until all required gates pass and rollback has been rehearsed.
