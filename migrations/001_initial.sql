-- Aivory Mail initial schema — compatible with Postgres and SQLite (via sqlx migrate)
-- Postgres version

CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS domains (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    domain TEXT UNIQUE NOT NULL,
    status TEXT NOT NULL DEFAULT 'Pending',
    dkim_selector TEXT NOT NULL DEFAULT 'aivory',
    sending_subdomain TEXT,
    cf_zone_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS mailboxes (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    domain_id UUID NOT NULL REFERENCES domains(id) ON DELETE CASCADE,
    address TEXT UNIQUE NOT NULL,
    display_name TEXT,
    is_catch_all BOOLEAN NOT NULL DEFAULT FALSE,
    forward_to TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS threads (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    mailbox_id UUID NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    subject TEXT,
    participant_addrs TEXT NOT NULL DEFAULT '[]',
    message_count INTEGER NOT NULL DEFAULT 0,
    last_message_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    has_unread BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    mailbox_id UUID NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    thread_id UUID REFERENCES threads(id) ON DELETE SET NULL,
    message_id TEXT NOT NULL,
    from_addr TEXT NOT NULL DEFAULT '',
    from_name TEXT,
    to_addrs TEXT NOT NULL DEFAULT '[]',
    cc_addrs TEXT NOT NULL DEFAULT '[]',
    subject TEXT,
    snippet TEXT,
    body_text TEXT,
    body_html TEXT,
    folder TEXT NOT NULL DEFAULT 'Inbox',
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    is_starred BOOLEAN NOT NULL DEFAULT FALSE,
    raw_r2_key TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    has_attachments BOOLEAN NOT NULL DEFAULT FALSE,
    headers_json JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_messages_mailbox ON messages(mailbox_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_messages_folder ON messages(folder);

CREATE TABLE IF NOT EXISTS attachments (
    id UUID PRIMARY KEY,
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    r2_key TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
