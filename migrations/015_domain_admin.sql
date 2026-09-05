-- Each domain has exactly one admin mailbox — the only account allowed into
-- the admin console and allowed to read/manage mailboxes other than its own
-- on that domain. Without this, "is admin" fell back to a single
-- instance-wide env var with no per-domain concept at all.
ALTER TABLE domains ADD COLUMN IF NOT EXISTS admin_email TEXT;
