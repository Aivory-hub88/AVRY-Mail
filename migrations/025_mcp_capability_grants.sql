CREATE TABLE IF NOT EXISTS mcp_capability_grants (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mailbox_id UUID NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    caller_id TEXT NOT NULL,
    audience TEXT NOT NULL,
    scopes TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    jti UUID NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_mcp_grants_mailbox_state
    ON mcp_capability_grants(tenant_id, mailbox_id, revoked_at);
CREATE INDEX IF NOT EXISTS idx_mcp_grants_expiry_state
    ON mcp_capability_grants(expires_at, revoked_at);
