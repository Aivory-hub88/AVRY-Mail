-- User settings for 10 Gmail-parity features
CREATE TABLE IF NOT EXISTS user_settings (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    mailbox_id TEXT,
    category TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(tenant_id, mailbox_id, category, key)
);
-- Filters/Rules
CREATE TABLE IF NOT EXISTS mail_filters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    criteria_json TEXT NOT NULL DEFAULT '{}',
    action_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
-- Labels
CREATE TABLE IF NOT EXISTS mail_labels (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#3b82f6',
    created_at TEXT NOT NULL
);
-- Vacation responder
CREATE TABLE IF NOT EXISTS vacation_responders (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    subject TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    start_at TEXT,
    end_at TEXT,
    interval_days INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL
);
-- Send As aliases
CREATE TABLE IF NOT EXISTS send_as_aliases (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    alias_email TEXT NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
-- Forwarding
CREATE TABLE IF NOT EXISTS forwarding_rules (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    forward_to TEXT NOT NULL,
    keep_copy INTEGER NOT NULL DEFAULT 1,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
