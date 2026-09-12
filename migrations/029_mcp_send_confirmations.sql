CREATE TABLE IF NOT EXISTS mcp_send_confirmations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mailbox_id UUID NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    caller_id TEXT NOT NULL,
    action TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_mcp_send_confirmations_scope
    ON mcp_send_confirmations(tenant_id, mailbox_id, caller_id, action, used_at);
CREATE INDEX IF NOT EXISTS idx_mcp_send_confirmations_expiry
    ON mcp_send_confirmations(expires_at, used_at);
