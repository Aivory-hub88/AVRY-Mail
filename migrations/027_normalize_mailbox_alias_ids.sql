-- Migration 024 historically declared mailbox_aliases foreign-key columns as
-- TEXT while domains.id and mailboxes.id are UUID. The bootstrap in DbPool
-- pre-creates a UUID-compatible table for fresh or partially migrated
-- databases; this forward migration also repairs installations where 024
-- completed with the legacy column types.
DO $$
DECLARE
    id_type TEXT;
    domain_type TEXT;
    mailbox_type TEXT;
BEGIN
    IF to_regclass('public.mailbox_aliases') IS NULL THEN
        CREATE TABLE mailbox_aliases (
            id UUID PRIMARY KEY,
            domain_id UUID NOT NULL,
            mailbox_id UUID NOT NULL,
            local_part TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(domain_id, local_part)
        );
    END IF;

    ALTER TABLE mailbox_aliases
        DROP CONSTRAINT IF EXISTS mailbox_aliases_domain_id_fkey,
        DROP CONSTRAINT IF EXISTS mailbox_aliases_mailbox_id_fkey;

    SELECT data_type INTO id_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'mailbox_aliases'
      AND column_name = 'id';
    IF id_type IS NOT NULL AND id_type <> 'uuid' THEN
        ALTER TABLE mailbox_aliases
            ALTER COLUMN id TYPE UUID USING id::uuid;
    END IF;

    SELECT data_type INTO domain_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'mailbox_aliases'
      AND column_name = 'domain_id';
    IF domain_type IS NOT NULL AND domain_type <> 'uuid' THEN
        ALTER TABLE mailbox_aliases
            ALTER COLUMN domain_id TYPE UUID USING domain_id::uuid;
    END IF;

    SELECT data_type INTO mailbox_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'mailbox_aliases'
      AND column_name = 'mailbox_id';
    IF mailbox_type IS NOT NULL AND mailbox_type <> 'uuid' THEN
        ALTER TABLE mailbox_aliases
            ALTER COLUMN mailbox_id TYPE UUID USING mailbox_id::uuid;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.mailbox_aliases'::regclass
          AND conname = 'mailbox_aliases_domain_id_fkey'
    ) THEN
        ALTER TABLE mailbox_aliases
            ADD CONSTRAINT mailbox_aliases_domain_id_fkey
            FOREIGN KEY (domain_id) REFERENCES domains(id) ON DELETE CASCADE;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.mailbox_aliases'::regclass
          AND conname = 'mailbox_aliases_mailbox_id_fkey'
    ) THEN
        ALTER TABLE mailbox_aliases
            ADD CONSTRAINT mailbox_aliases_mailbox_id_fkey
            FOREIGN KEY (mailbox_id) REFERENCES mailboxes(id) ON DELETE CASCADE;
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS idx_mailbox_aliases_mailbox
    ON mailbox_aliases(mailbox_id);
