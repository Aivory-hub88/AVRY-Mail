-- 016: Dovecot IMAP storage integration.
-- password_hash_dovecot: Dovecot-native {SHA512} hash for IMAP + SMTP
--   submission auth, populated alongside password_hash whenever a mailbox
--   password is set. Existing rows stay NULL until passwords are re-set.
-- maildir_file: relative path of the Maildir mirror copy
--   (<domain>/<user>/[.Sub/]{cur|new}/filename), written at delivery and
--   kept in sync for \Seen renames. Postgres stays the system of record.
ALTER TABLE mailboxes ADD COLUMN IF NOT EXISTS password_hash_dovecot TEXT;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS maildir_file TEXT;
