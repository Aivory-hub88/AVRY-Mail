-- Migration 017 historically declared email_integrations.mailbox_id as
-- TEXT while mailboxes.id is UUID. The bootstrap in DbPool pre-creates a
-- UUID-compatible table for fresh or partially migrated databases; this
-- forward migration repairs installations where 017 completed with legacy
-- column types.
DO $$
DECLARE
    id_type TEXT;
    mailbox_type TEXT;
BEGIN
    IF to_regclass('public.email_integrations') IS NULL THEN
        CREATE TABLE email_integrations (
            id UUID PRIMARY KEY,
            mailbox_id UUID NOT NULL UNIQUE,
            host TEXT NOT NULL,
            port INTEGER NOT NULL,
            username TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'connected',
            last_tested_at TEXT,
            last_connected_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
    END IF;

    ALTER TABLE email_integrations
        DROP CONSTRAINT IF EXISTS email_integrations_mailbox_id_fkey;

    SELECT data_type INTO id_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'email_integrations'
      AND column_name = 'id';
    IF id_type IS NOT NULL AND id_type <> 'uuid' THEN
        ALTER TABLE email_integrations
            ALTER COLUMN id TYPE UUID USING id::uuid;
    END IF;

    SELECT data_type INTO mailbox_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'email_integrations'
      AND column_name = 'mailbox_id';
    IF mailbox_type IS NOT NULL AND mailbox_type <> 'uuid' THEN
        ALTER TABLE email_integrations
            ALTER COLUMN mailbox_id TYPE UUID USING mailbox_id::uuid;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.email_integrations'::regclass
          AND conname = 'email_integrations_mailbox_id_fkey'
    ) THEN
        ALTER TABLE email_integrations
            ADD CONSTRAINT email_integrations_mailbox_id_fkey
            FOREIGN KEY (mailbox_id) REFERENCES mailboxes(id) ON DELETE CASCADE;
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS idx_email_integrations_mailbox
    ON email_integrations(mailbox_id);
