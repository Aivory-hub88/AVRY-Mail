-- 032: CC/BCC persisted on drafts (draft.create accepts + stores them).
ALTER TABLE messages ADD COLUMN IF NOT EXISTS cc_addrs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE messages ADD COLUMN IF NOT EXISTS bcc_addrs TEXT NOT NULL DEFAULT '[]';
