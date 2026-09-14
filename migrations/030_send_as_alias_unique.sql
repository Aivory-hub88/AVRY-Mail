-- Migration 030: one alias_email per mailbox (case-insensitive).
-- Legacy installs accumulated exact duplicates (e.g. cs@ twice on the same
-- mailbox) because creation never checked. Keep the earliest row per
-- (mailbox_id, lower(alias_email)), then enforce uniqueness going forward.
-- Portable: runs on Postgres and SQLite (migrate() covers both).
DELETE FROM send_as_aliases WHERE id IN (
    SELECT id FROM (
        SELECT id, ROW_NUMBER() OVER (
            PARTITION BY mailbox_id, lower(alias_email)
            ORDER BY created_at ASC
        ) AS rn
        FROM send_as_aliases
    ) AS ranked
    WHERE rn > 1
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_send_as_alias_mailbox_email
    ON send_as_aliases (mailbox_id, lower(alias_email));
