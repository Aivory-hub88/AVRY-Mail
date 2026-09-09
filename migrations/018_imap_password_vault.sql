-- 018: Reversible IMAP-password vault for authorized administrative recovery.
-- Dovecot continues to authenticate only against password_hash_dovecot. This
-- separate value contains a versioned AES-256-GCM ciphertext and is bound to
-- the mailbox UUID as authenticated additional data by the application.
ALTER TABLE mailboxes ADD COLUMN IF NOT EXISTS imap_password_encrypted TEXT;
