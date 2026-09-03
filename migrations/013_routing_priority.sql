-- 013 routing priority + reject/block polish
ALTER TABLE mail_filters ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_mail_filters_priority ON mail_filters(tenant_id, priority, created_at);

-- Webhooks registry (Mailflare parity: webhook management UI + retry visibility)
CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    url TEXT NOT NULL,
    events TEXT NOT NULL DEFAULT '["email.received"]',
    secret TEXT NOT NULL DEFAULT '',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_webhooks_tenant ON webhooks(tenant_id);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id UUID PRIMARY KEY,
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_retry_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook ON webhook_deliveries(webhook_id, created_at DESC);

-- Agent task queue (Mailflare agent inbox view: needs_reply/waiting/FYI/auto-handled/needs_approval)
CREATE TABLE IF NOT EXISTS agent_tasks (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    mailbox_id TEXT,
    thread_id TEXT,
    message_id TEXT,
    type TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'needs_reply',
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_state ON agent_tasks(tenant_id, state, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_mailbox ON agent_tasks(mailbox_id);
