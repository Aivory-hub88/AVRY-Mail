-- Custom folders per-mailbox (Mailflare parity: folders with color)
CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    mailbox_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#006355',
    created_at TEXT NOT NULL,
    UNIQUE(mailbox_id, name)
);
CREATE INDEX IF NOT EXISTS idx_folders_mailbox ON folders(mailbox_id);
