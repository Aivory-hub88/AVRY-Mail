-- Migration 002 historically declared signatures.id/mailbox_id as TEXT while
-- the canonical Postgres mailbox identity is UUID. The bootstrap in DbPool
-- pre-creates a UUID-compatible table for fresh or partially migrated databases
-- so migration 002 remains checksum-stable and its IF NOT EXISTS is harmless.
-- This forward migration also normalizes installations where 002 completed.
DO $$
DECLARE
    id_type TEXT;
    mailbox_type TEXT;
    default_type TEXT;
BEGIN
    IF to_regclass('public.signatures') IS NULL THEN
        CREATE TABLE signatures (
            id UUID PRIMARY KEY,
            tenant_id TEXT NOT NULL DEFAULT '',
            mailbox_id UUID NOT NULL,
            name TEXT NOT NULL DEFAULT 'Default',
            html TEXT NOT NULL DEFAULT '',
            text TEXT NOT NULL DEFAULT '',
            is_default BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TEXT NOT NULL
        );
    END IF;

    ALTER TABLE signatures DROP CONSTRAINT IF EXISTS signatures_mailbox_id_fkey;

    SELECT data_type INTO id_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'signatures'
      AND column_name = 'id';
    IF id_type IS NOT NULL AND id_type <> 'uuid' THEN
        ALTER TABLE signatures
            ALTER COLUMN id TYPE UUID USING id::uuid;
    END IF;

    SELECT data_type INTO mailbox_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'signatures'
      AND column_name = 'mailbox_id';
    IF mailbox_type IS NOT NULL AND mailbox_type <> 'uuid' THEN
        ALTER TABLE signatures
            ALTER COLUMN mailbox_id TYPE UUID USING mailbox_id::uuid;
    END IF;

    SELECT data_type INTO default_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'signatures'
      AND column_name = 'is_default';
    IF default_type IS NOT NULL AND default_type <> 'boolean' THEN
        ALTER TABLE signatures
            ALTER COLUMN is_default DROP DEFAULT;
        IF default_type IN ('smallint', 'integer', 'bigint') THEN
            ALTER TABLE signatures
                ALTER COLUMN is_default TYPE BOOLEAN USING (is_default <> 0);
        ELSE
            ALTER TABLE signatures
                ALTER COLUMN is_default TYPE BOOLEAN
                USING (lower(is_default::text) IN ('true', 't', '1', 'yes'));
        END IF;
    END IF;
    IF default_type IS NOT NULL THEN
        ALTER TABLE signatures
            ALTER COLUMN is_default SET DEFAULT FALSE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.signatures'::regclass
          AND conname = 'signatures_mailbox_id_fkey'
    ) THEN
        ALTER TABLE signatures
            ADD CONSTRAINT signatures_mailbox_id_fkey
            FOREIGN KEY (mailbox_id) REFERENCES mailboxes(id) ON DELETE CASCADE;
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS idx_signatures_mailbox
    ON signatures(mailbox_id);
