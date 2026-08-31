-- Real domain ownership verification + per-domain DKIM signing key material.
-- Postgres version (sqlite bootstrap mirror lives in main.rs::ensure_schema).

ALTER TABLE domains ADD COLUMN IF NOT EXISTS verification_token TEXT;
ALTER TABLE domains ADD COLUMN IF NOT EXISTS dkim_public_key TEXT;
ALTER TABLE domains ADD COLUMN IF NOT EXISTS dkim_private_key TEXT;
ALTER TABLE domains ADD COLUMN IF NOT EXISTS failure_reason TEXT;
