-- 017: Email Account integration (Settings > Integrations > Email Account)
-- Self-service IMAP credential for every mailbox (was admin-only).
-- After user saves (host/port/username + password), we hash the password into
-- mailboxes.password_hash_dovecot and keep a small status row here.
-- The plaintext password is never stored or rendered again — UI only shows
-- "Connected as user@domain.com" with disconnect/reconnect.
-- Admin can check status (host/port/connected) via the admin endpoint
-- without ever seeing the password.
CREATE TABLE IF NOT EXISTS email_integrations (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL UNIQUE REFERENCES mailboxes(id) ON DELETE CASCADE,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'connected',
    last_tested_at TEXT,
    last_connected_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_email_integrations_mailbox ON email_integrations(mailbox_id);
