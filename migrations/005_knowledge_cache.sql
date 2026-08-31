-- Knowledge compiler cache for agent (auto-compile per tenant/scope)
CREATE TABLE IF NOT EXISTS knowledge_cache (
    tenant_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    compiled_json TEXT NOT NULL,
    cursor TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, scope)
);
CREATE INDEX IF NOT EXISTS idx_knowledge_cache_updated ON knowledge_cache(updated_at);
