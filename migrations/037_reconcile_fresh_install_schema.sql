-- 037: make a database built from migrations 001-036 match the schema the
-- code actually runs against (production's).
--
-- Production was reshaped by hand over time (tables created outside the
-- migrations, column types changed), and the API's queries are written for
-- that shape: `use_all_domains != 0` on an INTEGER column, string ids bound to
-- contacts.id, a `groups` table, `api_keys.key_raw`. A database built purely
-- from the migrations had none of that, so it booted but then failed at
-- runtime ("operator does not exist: boolean <> integer" on the first inbound
-- mail, no groups tables, no default tenant).
--
-- Every statement is guarded by the current catalog state, so on production,
-- where each target already holds, this migration changes nothing.
-- Cosmetic differences (jsonb NOT NULL/DEFAULT on ai_chat_history,
-- mission_control_notifications, webhook_deliveries) don't affect behaviour
-- because the code always supplies those values, and are left alone.

-- A fresh install has no tenant, and every domain row needs one. The API
-- creates domains under the nil UUID.
INSERT INTO tenants (id, slug, name)
SELECT '00000000-0000-0000-0000-000000000000', 'default', 'Default'
WHERE NOT EXISTS (SELECT 1 FROM tenants);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    UNIQUE(tenant_id, email)
);

CREATE TABLE IF NOT EXISTS group_members (
    group_id TEXT NOT NULL,
    mailbox_id TEXT NOT NULL,
    PRIMARY KEY (group_id, mailbox_id)
);

ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS key_raw TEXT NOT NULL DEFAULT '';

DO $$
DECLARE
    -- type of public.<tbl>.<col>, NULL when the table or column is absent
    t TEXT;
BEGIN
    -- contacts: id UUID, timestamps timestamptz, blocked boolean
    IF to_regclass('public.contacts') IS NOT NULL THEN
        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'contacts' AND column_name = 'id';
        IF t = 'text' THEN
            ALTER TABLE contacts ALTER COLUMN id TYPE UUID USING id::uuid;
        END IF;

        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'contacts' AND column_name = 'created_at';
        IF t = 'text' THEN
            ALTER TABLE contacts ALTER COLUMN created_at DROP DEFAULT;
            ALTER TABLE contacts ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at::timestamptz;
            ALTER TABLE contacts ALTER COLUMN created_at SET DEFAULT NOW();
        END IF;

        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'contacts' AND column_name = 'last_seen_at';
        IF t = 'text' THEN
            ALTER TABLE contacts ALTER COLUMN last_seen_at DROP DEFAULT;
            ALTER TABLE contacts ALTER COLUMN last_seen_at TYPE TIMESTAMPTZ USING last_seen_at::timestamptz;
            ALTER TABLE contacts ALTER COLUMN last_seen_at SET DEFAULT NOW();
        END IF;

        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'contacts' AND column_name = 'blocked';
        IF t = 'integer' THEN
            ALTER TABLE contacts ALTER COLUMN blocked DROP DEFAULT;
            ALTER TABLE contacts ALTER COLUMN blocked TYPE BOOLEAN USING (blocked <> 0);
            ALTER TABLE contacts ALTER COLUMN blocked SET DEFAULT FALSE;
        END IF;
    END IF;

    -- mail_filters: id UUID, created_at timestamptz, enabled boolean
    IF to_regclass('public.mail_filters') IS NOT NULL THEN
        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'mail_filters' AND column_name = 'id';
        IF t = 'text' THEN
            ALTER TABLE mail_filters ALTER COLUMN id TYPE UUID USING id::uuid;
        END IF;

        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'mail_filters' AND column_name = 'created_at';
        IF t = 'text' THEN
            ALTER TABLE mail_filters ALTER COLUMN created_at DROP DEFAULT;
            ALTER TABLE mail_filters ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at::timestamptz;
            ALTER TABLE mail_filters ALTER COLUMN created_at SET DEFAULT NOW();
        END IF;

        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'mail_filters' AND column_name = 'enabled';
        IF t = 'integer' THEN
            ALTER TABLE mail_filters ALTER COLUMN enabled DROP DEFAULT;
            ALTER TABLE mail_filters ALTER COLUMN enabled TYPE BOOLEAN USING (enabled <> 0);
            ALTER TABLE mail_filters ALTER COLUMN enabled SET DEFAULT TRUE;
        END IF;
    END IF;

    -- mailboxes.use_all_domains: the code compares it with 0
    SELECT data_type INTO t FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'mailboxes' AND column_name = 'use_all_domains';
    IF t = 'boolean' THEN
        ALTER TABLE mailboxes ALTER COLUMN use_all_domains DROP DEFAULT;
        ALTER TABLE mailboxes ALTER COLUMN use_all_domains TYPE INTEGER
            USING (CASE WHEN use_all_domains THEN 1 ELSE 0 END);
        ALTER TABLE mailboxes ALTER COLUMN use_all_domains SET DEFAULT 0;
    END IF;

    -- message_labels.message_id: the code binds string ids, and production
    -- has no foreign keys on this table.
    IF to_regclass('public.message_labels') IS NOT NULL THEN
        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'message_labels' AND column_name = 'message_id';
        IF t = 'uuid' THEN
            ALTER TABLE message_labels DROP CONSTRAINT IF EXISTS message_labels_message_id_fkey;
            ALTER TABLE message_labels DROP CONSTRAINT IF EXISTS message_labels_label_id_fkey;
            ALTER TABLE message_labels ALTER COLUMN message_id TYPE TEXT USING message_id::text;
        END IF;
    END IF;

    -- vacation_replies_sent.sent_at: stored as text
    IF to_regclass('public.vacation_replies_sent') IS NOT NULL THEN
        SELECT data_type INTO t FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'vacation_replies_sent' AND column_name = 'sent_at';
        IF t = 'timestamp with time zone' THEN
            ALTER TABLE vacation_replies_sent ALTER COLUMN sent_at DROP DEFAULT;
            ALTER TABLE vacation_replies_sent ALTER COLUMN sent_at TYPE TEXT USING sent_at::text;
        END IF;
    END IF;
END
$$;
