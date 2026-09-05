-- 014: real per-mailbox password (admin console "Create account" was silently
-- reusing the shared MAIL_ADMIN_PASSWORD for every new mailbox — no way to set
-- a per-account password from the UI).
ALTER TABLE mailboxes ADD COLUMN IF NOT EXISTS password_hash TEXT;
