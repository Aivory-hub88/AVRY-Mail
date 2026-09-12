# Staging MCP Capability E2E

This runbook describes the opt-in live HTTP regression test for the MCP v2 capability boundary. It is intentionally separate from production deployment and must use an isolated staging API, database, network, storage volume, credentials, and synthetic mail data.

## Safety rules

- Never point this test at `mail.aivory.uk`, the production API, `avry-postgres`, `avry-mail_maildir`, `avry-mail_mail_storage`, or the production `aivory-network`.
- Never export production JWT, internal, database, IMAP-encryption, or admin credentials into the test shell.
- The test does not call `send_mail`, Cloudflare, SMTP, OpenRouter, Cerveau, or n8n. It only performs health, admin grant lifecycle, MCP read/list/search, expiry, revocation, and cleanup checks.
- The test refuses known production Aivory hosts, requires `STAGING_API_URL` to match the exact `STAGING_ALLOWED_HOST`, disables HTTP redirects, and requires `STAGING_ALLOW_REMOTE=1` for non-loopback hosts.
- `AVRY_MCP_CAPABILITY_MODE=v2` belongs in staging only. Production remains unset/disabled until a separate approved canary.
- Do not use a shared or production Postgres database for `TEST_DATABASE_URL` or any staging test.

## Staging topology

The reference isolated staging stack uses unique resources:

| Resource | Reference value |
|---|---|
| API container | `avry-mail-staging` |
| Postgres container | `avry-postgres-staging` |
| Docker network | `avry-mail-staging-net` |
| Postgres volume | `avry-mail-staging-pgdata` |
| API storage volume | `avry-mail-staging-storage` |
| Host bind | `127.0.0.1:18095 -> 8095` |
| MCP mode | `AVRY_MCP_CAPABILITY_MODE=v2` |

The server-side staging configuration must explicitly pass `AVRY_MCP_CAPABILITY_MODE=v2` to `avry-mail-staging`. Verify the named resources and mode before exporting credentials:

```bash
for name in avry-mail-staging avry-postgres-staging; do
  sudo docker inspect "$name" >/dev/null
 done
sudo docker network inspect avry-mail-staging-net >/dev/null
mode=$(sudo docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' avry-mail-staging \
  | awk -F= '$1=="AVRY_MCP_CAPABILITY_MODE" {print $2}')
test "$mode" = v2
curl -fsS http://127.0.0.1:18095/health
```

The API must use the isolated Postgres and storage resources listed above. Do not proceed if any named resource is missing, points at a production network/volume, or does not report MCP mode `v2`.

The staging database must contain at least two synthetic tenants, two domains, and two mailboxes with valid tenant/domain/mailbox UUID relationships. Seed one unique, non-sensitive subject sentinel per mailbox. The test searches the subject sentinels because the MCP search response intentionally returns message ID, subject, and sender rather than the snippet field.

## Test prerequisites

From the AVRY-Mail repository root:

```bash
cargo check -p aivory-mail-api
```

The live test requires a staging admin identity, two staging mailbox relationships, and the two seeded subject sentinels. Set them only in the current shell; do not write them to the repository or a production `.env` file:

```bash
export STAGING_E2E=1
export STAGING_API_URL=http://127.0.0.1:18095
export STAGING_ALLOWED_HOST=127.0.0.1
# Only set this for an explicitly approved non-loopback staging host.
# export STAGING_ALLOW_REMOTE=1
export STAGING_ADMIN_EMAIL=staging-admin@example.invalid
read -r -s STAGING_ADMIN_PASSWORD
export STAGING_ADMIN_PASSWORD
export STAGING_TENANT_ID=<staging-tenant-a-uuid>
export STAGING_MAILBOX_ID=<staging-mailbox-a-uuid>
export STAGING_FOREIGN_TENANT_ID=<staging-tenant-b-uuid>
export STAGING_FOREIGN_MAILBOX_ID=<staging-mailbox-b-uuid>
export STAGING_AGENT_ID=phase5-staging-agent
export STAGING_OWN_SENTINEL='Synthetic A isolation marker'
export STAGING_FOREIGN_SENTINEL='Synthetic B isolation marker'
```

The password is read silently and is not echoed by the shell. The test process receives it only through `STAGING_ADMIN_PASSWORD`.

## Run the E2E regression test

The test is ignored by default and has a second explicit environment guard. Normal workspace tests therefore cannot contact a network endpoint:

```bash
STAGING_E2E=1 cargo test \
  -p aivory-mail-api \
  --test staging_mcp_e2e \
  --ignored \
  --nocapture
```

The test performs the following checks:

1. Staging health returns HTTP 200 and `status=ok`.
2. Staging admin login succeeds and returns an admin JWT.
3. A short-lived mailbox-scoped capability can call `tools/list`, `search_mail`, and `get_inbox_overview` with valid JSON-RPC result envelopes.
4. Mailbox A can see its own sentinel, even when forged tenant/mailbox tool arguments are supplied, but cannot retrieve mailbox B's sentinel; the foreign result must be an actual empty result.
5. A `mail.search`-only capability cannot call the `mail.read` overview tool, and a `mail.read`-only capability cannot search.
6. Read-only capabilities cannot call thread-memory, knowledge, or send tools because their required scopes are absent.
7. Invalid scope and wrong audience requests are rejected with HTTP 400.
8. Tenant/mailbox mismatches are rejected with HTTP 403.
9. Wrong `x-agent-id` is rejected with HTTP 401 or 403, while the current contract's optional caller-header behavior is explicitly verified.
10. Unknown bearer tokens, internal-token, Cerveau-secret, and legacy query-key bypass attempts are rejected with HTTP 401.
11. An admin JWT cannot be used as an MCP capability.
12. An expired capability is rejected with HTTP 401.
13. A revoked capability is rejected with HTTP 401.
14. All grants created by the run's unique caller ID are revoked and verified absent, including after individual delete failures where possible.

The test never prints access tokens, admin credentials, response bodies containing credentials, or production identifiers. Unset the staging variables after the run:

```bash
unset STAGING_E2E STAGING_API_URL STAGING_ALLOWED_HOST STAGING_ALLOW_REMOTE \
  STAGING_ADMIN_EMAIL STAGING_ADMIN_PASSWORD \
  STAGING_TENANT_ID STAGING_MAILBOX_ID STAGING_FOREIGN_TENANT_ID \
  STAGING_FOREIGN_MAILBOX_ID STAGING_AGENT_ID STAGING_OWN_SENTINEL \
  STAGING_FOREIGN_SENTINEL
```

## Cleanup after interruption

The test cleans up on normal assertion failure as well as success by listing grants as the staging admin and deleting only grants whose `caller_id` equals the run-specific caller. Cleanup retries the grant listing, attempts every matching deletion while collecting failures, and performs a final verification. If the process is terminated before cleanup, do not delete all grants blindly. List the staging grants, identify only the test caller prefix, revoke those IDs through `/v1/agent-access/grants/:id`, and verify the count is zero.

If the entire staging stack is disposable and no further regression run is needed, remove only the named staging resources after confirming the container and volume names:

```bash
sudo docker rm -f avry-mail-staging avry-postgres-staging
sudo docker network rm avry-mail-staging-net
sudo docker volume rm avry-mail-staging-pgdata avry-mail-staging-storage
```

This teardown is destructive to synthetic staging data and must never be run with production resource names.

## Controlled production rollback pattern

Production MCP must remain disabled unless a separate approved canary is in progress. Before any production canary:

1. Run the migration integrity preflight and verify the deployed database migration ledger read-only.
2. Create and checksum a fresh production database backup.
3. Confirm the base production compose configuration contains no MCP v2 enablement.
4. Confirm the base `.env` admin credential matches the currently approved admin credential; a temporary compose override must not silently replace it during a container recreation.
5. Use a temporary, permission-restricted compose override to set only `AVRY_MCP_CAPABILITY_MODE=v2`.
6. Run one short-lived, read-only, mailbox-scoped capability call.
7. Verify invalid capability, admin JWT, wrong caller, expired/revoked capability, health, migrations, and grant cleanup.
8. Recreate the API from the base compose configuration, verify `AVRY_MCP_CAPABILITY_MODE` is unset/disabled, and verify health again.
9. Remove the temporary override and retain the backup and checksum.

If any canary assertion fails, revoke/delete only the canary grant, restore the base compose configuration immediately, and verify the API health endpoint and migration ledger before investigating further. Never enable MCP v2 permanently as part of this test command.

## Validation commands

Local tests remain safe and do not need staging or production credentials:

```bash
cargo test
cargo check
make migration-check
```

The optional `TEST_DATABASE_URL` Postgres parity test mutates its database and deletes its generated rows. Use only a disposable isolated database; it is not a substitute for the live staging test above. `make migration-check` is a useful advisory preflight, but this checkout documents historical duplicate migration prefixes; do not treat that known baseline as a staging E2E failure or rewrite migration history to make it pass.
