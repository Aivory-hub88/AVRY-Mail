use aivory_mail_api::{
    api::{self, mcp_capabilities, mcp_confirmations, AppState},
    config::Config,
    realtime::RealtimeHub,
};
use aivory_mail_storage::{
    db::DbPool,
    object_store::{LocalStore, ObjectStore},
};
use aivory_mail_core::types::SendRequest;
use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderName, HeaderValue, Method, Request, StatusCode},
    Router,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    state: Arc<AppState>,
    tenant_a: Uuid,
    tenant_b: Uuid,
    mailbox_a: Uuid,
    mailbox_b: Uuid,
    message_a: Uuid,
    message_b: Uuid,
}

async fn fixture() -> Fixture {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("sqlite fixture pool");
    let db = DbPool::Sqlite(pool);

    for statement in [
        "CREATE TABLE tenants (id TEXT PRIMARY KEY, slug TEXT NOT NULL, name TEXT NOT NULL, created_at TEXT NOT NULL)",
        "CREATE TABLE mailboxes (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, domain_id TEXT NOT NULL, address TEXT UNIQUE NOT NULL, created_at TEXT NOT NULL)",
        "CREATE TABLE messages (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, thread_id TEXT, message_id TEXT NOT NULL, from_addr TEXT NOT NULL, subject TEXT, snippet TEXT, body_text TEXT, folder TEXT NOT NULL, is_read INTEGER NOT NULL DEFAULT 0, snoozed_until TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE threads (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, subject TEXT, last_message_at TEXT NOT NULL)",
        "CREATE TABLE knowledge_cache (tenant_id TEXT NOT NULL, scope TEXT NOT NULL, compiled_json TEXT NOT NULL, cursor TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY (tenant_id, scope))",
        "CREATE TABLE mcp_capability_grants (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, caller_id TEXT NOT NULL, audience TEXT NOT NULL, scopes TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, jti TEXT NOT NULL UNIQUE, expires_at TEXT NOT NULL, revoked_at TEXT, last_used_at TEXT, created_at TEXT NOT NULL)",
        "CREATE TABLE mcp_send_confirmations (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, mailbox_id TEXT NOT NULL, caller_id TEXT NOT NULL, action TEXT NOT NULL, payload_hash TEXT NOT NULL, expires_at TEXT NOT NULL, issued_at TEXT NOT NULL, used_at TEXT)",
        "CREATE TABLE domains (id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, domain TEXT UNIQUE NOT NULL, status TEXT NOT NULL, dkim_private_key TEXT)",
    ] {
        execute_sql(&db, statement).await;
    }

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let mailbox_a = Uuid::new_v4();
    let mailbox_b = Uuid::new_v4();
    let message_a = Uuid::new_v4();
    let message_b = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();

    execute_sql(
        &db,
        &format!(
            "INSERT INTO tenants (id, slug, name, created_at) VALUES ('{tenant_a}', 'tenant-a', 'Tenant A', '{now}'), ('{tenant_b}', 'tenant-b', 'Tenant B', '{now}')"
        ),
    )
    .await;
    execute_sql(
        &db,
        &format!(
            "INSERT INTO mailboxes (id, tenant_id, domain_id, address, created_at) VALUES ('{mailbox_a}', '{tenant_a}', 'domain-a', 'agent-a@test.local', '{now}'), ('{mailbox_b}', '{tenant_b}', 'domain-b', 'agent-b@test.local', '{now}')"
        ),
    )
    .await;
    execute_sql(
        &db,
        &format!(
            "INSERT INTO messages (id, tenant_id, mailbox_id, message_id, from_addr, subject, snippet, body_text, folder, created_at) VALUES ('{message_a}', '{tenant_a}', '{mailbox_a}', 'msg-a', 'sender-a@test.local', 'A secret', 'tenant A only', 'tenant A body', 'Inbox', '{now}'), ('{message_b}', '{tenant_b}', '{mailbox_b}', 'msg-b', 'sender-b@test.local', 'B secret', 'tenant B only', 'tenant B body', 'Inbox', '{now}')"
        ),
    )
    .await;
    execute_sql(
        &db,
        &format!(
            "INSERT INTO domains (id, tenant_id, domain, status, dkim_private_key) VALUES ('{}', '{tenant_a}', 'test.local', 'Active', 'test-dkim-key')",
            Uuid::new_v4()
        ),
    )
    .await;

    let store: Arc<dyn ObjectStore> = Arc::new(LocalStore::new(
        std::env::temp_dir().join("aivory-mail-phase2-tests"),
    ));
    let state = Arc::new(AppState {
        config: Config::for_tests("sqlite::memory:"),
        db,
        store,
        hub: RealtimeHub::new(),
    });

    Fixture {
        state,
        tenant_a,
        tenant_b,
        mailbox_a,
        mailbox_b,
        message_a,
        message_b,
    }
}

async fn execute_sql(db: &DbPool, statement: &str) {
    match db {
        DbPool::Sqlite(pool) => sqlx::query(statement)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("fixture SQL failed: {error}\n{statement}")),
        DbPool::Postgres(_) => unreachable!("fixture is SQLite-only"),
    };
}

async fn issue(
    fixture: &Fixture,
    tenant_id: Uuid,
    mailbox_id: Uuid,
    scopes: &[&str],
    expires_in_seconds: Option<i64>,
) -> (mcp_capabilities::CapabilityMetadata, String) {
    mcp_capabilities::issue_capability(
        &fixture.state,
        mcp_capabilities::IssueCapabilityRequest {
            tenant_id: tenant_id.to_string(),
            mailbox_id: mailbox_id.to_string(),
            caller_id: "zeroclaw-mail-assistant".to_string(),
            audience: mcp_capabilities::MCP_AUDIENCE.to_string(),
            scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
            expires_in_seconds,
        },
    )
    .await
    .expect("capability issuance")
}

async fn mcp_request(
    app: &Router,
    token: Option<&str>,
    body: Value,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    mcp_request_at(app, "/mcp", token, body, extra_headers).await
}

async fn mcp_request_at(
    app: &Router,
    path: &str,
    token: Option<&str>,
    body: Value,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut request = Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("MCP request");
    if let Some(token) = token {
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().expect("bearer header"),
        );
    }
    for (name, value) in extra_headers {
        request.headers_mut().insert(
            HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            HeaderValue::from_str(value).expect("header value"),
        );
    }
    let response = app.clone().oneshot(request).await.expect("MCP response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("MCP response body");
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({}));
    (status, value)
}

fn admin_token(state: &AppState, subject: &str) -> String {
    let claims = aivory_mail_api::auth::Claims {
        sub: subject.to_string(),
        tenant_id: None,
        role: Some("admin".to_string()),
        exp: (chrono::Utc::now() + chrono::Duration::minutes(10)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .expect("admin test JWT")
}

async fn admin_request(
    app: &Router,
    method: Method,
    path: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let request = builder
        .body(Body::from(
            body.unwrap_or_else(|| serde_json::json!({})).to_string(),
        ))
        .expect("admin request");
    let response = app.clone().oneshot(request).await.expect("admin response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("admin response body");
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({}));
    (status, value)
}

fn token_hash(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn contains_identity_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            key == "mailbox_id" || key == "tenant_id" || contains_identity_key(value)
        }),
        Value::Array(values) => values.iter().any(contains_identity_key),
        _ => false,
    }
}

#[tokio::test]
async fn sqlite_capability_isolation_rejects_adversarial_identity_and_credentials() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.search", "mail.read"],
        None,
    )
    .await;

    let (status, response) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "search_mail",
                "arguments": {
                    "query": "secret",
                    "mailbox_id": fixture.mailbox_b,
                    "tenant_id": fixture.tenant_b
                }
            }
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("search text");
    assert!(text.contains(&fixture.message_a.to_string()));
    assert!(!text.contains(&fixture.message_b.to_string()));
    assert!(text.contains("A secret"));
    assert!(!text.contains("B secret"));

    let (status, tools) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!contains_identity_key(&tools));
    assert_eq!(
        tools["result"]["tools"][0]["annotations"]["readOnlyHint"],
        true
    );
    assert_eq!(
        tools["result"]["tools"][4]["annotations"]["idempotentHint"],
        false
    );

    let (status, overview) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"get_inbox_overview","arguments":{"mailbox_id":fixture.mailbox_b}}}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let overview_text = overview["result"]["content"][0]["text"]
        .as_str()
        .expect("overview text");
    assert!(overview_text.contains("\"total\":1"));

    let (status, _) = mcp_request(
        &app,
        None,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"initialize"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = mcp_request(
        &app,
        None,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"initialize"}),
        &[("x-internal-token", "test-internal-token")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = mcp_request_at(
        &app,
        "/mcp?api_key=legacy-global-key",
        None,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"initialize"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"search_mail","arguments":{"query":"secret"}}}),
        &[("x-agent-id", "wrong-agent")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn sqlite_grants_enforce_binding_scope_audience_expiry_and_revocation() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;

    let mismatch = mcp_capabilities::issue_capability(
        &fixture.state,
        mcp_capabilities::IssueCapabilityRequest {
            tenant_id: fixture.tenant_b.to_string(),
            mailbox_id: fixture.mailbox_a.to_string(),
            caller_id: "zeroclaw-mail-assistant".to_string(),
            audience: mcp_capabilities::MCP_AUDIENCE.to_string(),
            scopes: vec!["mail.search".to_string()],
            expires_in_seconds: None,
        },
    )
    .await;
    assert!(matches!(mismatch, Err(StatusCode::FORBIDDEN)));

    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.read"],
        None,
    )
    .await;
    assert_eq!(
        mcp_capabilities::validate_capability(
            &fixture.state,
            "unknown-token",
            mcp_capabilities::MCP_AUDIENCE,
            None,
        )
        .await,
        Err(StatusCode::UNAUTHORIZED)
    );
    assert_eq!(
        mcp_capabilities::validate_capability(&fixture.state, &token, "wrong-audience", None,)
            .await,
        Err(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        mcp_capabilities::validate_capability(
            &fixture.state,
            &token,
            mcp_capabilities::MCP_AUDIENCE,
            Some("wrong-agent"),
        )
        .await,
        Err(StatusCode::FORBIDDEN)
    );

    let app = api::router(fixture.state.clone());
    let (status, _) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"search_mail","arguments":{"query":"secret"}}}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, expiring_token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.search"],
        Some(1),
    )
    .await;
    let past = (chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339();
    execute_sql(
        &fixture.state.db,
        &format!(
            "UPDATE mcp_capability_grants SET expires_at='{past}' WHERE token_hash='{}'",
            token_hash(&expiring_token)
        ),
    )
    .await;
    assert_eq!(
        mcp_capabilities::validate_capability(
            &fixture.state,
            &expiring_token,
            mcp_capabilities::MCP_AUDIENCE,
            None,
        )
        .await,
        Err(StatusCode::UNAUTHORIZED)
    );

    let (_, revocable_token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.search"],
        None,
    )
    .await;
    let grant_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM mcp_capability_grants WHERE expires_at > datetime('now') ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(match &fixture.state.db {
        DbPool::Sqlite(pool) => pool,
        DbPool::Postgres(_) => unreachable!(),
    })
    .await
    .expect("revocable grant");
    mcp_capabilities::revoke_capability(&fixture.state, Uuid::parse_str(&grant_id).unwrap())
        .await
        .expect("revoke capability");
    assert_eq!(
        mcp_capabilities::validate_capability(
            &fixture.state,
            &revocable_token,
            mcp_capabilities::MCP_AUDIENCE,
            None,
        )
        .await,
        Err(StatusCode::UNAUTHORIZED)
    );
}

#[tokio::test]
async fn sqlite_admin_grant_lifecycle_returns_raw_token_once_and_revokes_it() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let admin = admin_token(&fixture.state, "admin@test.local");
    let user = admin_token(&fixture.state, "user@test.local");

    let (status, issued) = admin_request(
        &app,
        Method::POST,
        "/v1/agent-access/grants",
        &admin,
        Some(serde_json::json!({
            "tenant_id": fixture.tenant_a,
            "mailbox_id": fixture.mailbox_a,
            "caller_id": "zeroclaw-mail-assistant",
            "audience": mcp_capabilities::MCP_AUDIENCE,
            "scopes": ["mail.search"],
            "expires_in_seconds": 900
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let raw_token = issued["access_token"].as_str().expect("issued raw token");
    assert!(!raw_token.is_empty());
    let grant_id = issued["grant"]["id"]
        .as_str()
        .expect("issued grant id")
        .to_string();

    let (status, listed) =
        admin_request(&app, Method::GET, "/v1/agent-access/grants", &admin, None).await;
    assert_eq!(status, StatusCode::OK);
    let listed_text = listed.to_string();
    assert!(!listed_text.contains(raw_token));
    assert!(listed_text.contains(&grant_id));
    assert!(!listed_text.contains("access_token"));

    let (status, _) = admin_request(
        &app,
        Method::DELETE,
        &format!("/v1/agent-access/grants/{grant_id}"),
        &admin,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        mcp_capabilities::validate_capability(
            &fixture.state,
            raw_token,
            mcp_capabilities::MCP_AUDIENCE,
            None,
        )
        .await,
        Err(StatusCode::UNAUTHORIZED)
    );

    let (status, _) = admin_request(
        &app,
        Method::POST,
        "/v1/agent-access/grants",
        &user,
        Some(serde_json::json!({
            "tenant_id": fixture.tenant_a,
            "mailbox_id": fixture.mailbox_a,
            "caller_id": "zeroclaw-mail-assistant",
            "audience": mcp_capabilities::MCP_AUDIENCE,
            "scopes": ["mail.search"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn postgres_capability_parity_is_opt_in_and_isolated() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("skipping postgres parity test: TEST_DATABASE_URL is not set");
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("TEST_DATABASE_URL must point to a reachable test Postgres database");
    let db = DbPool::Postgres(pool.clone());
    db.migrate()
        .await
        .expect("Postgres test database migrations");

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let domain_a = Uuid::new_v4();
    let domain_b = Uuid::new_v4();
    let mailbox_a = Uuid::new_v4();
    let mailbox_b = Uuid::new_v4();
    let state = Arc::new(AppState {
        config: Config::for_tests(&database_url),
        db,
        store: Arc::new(LocalStore::new(
            std::env::temp_dir().join(format!("aivory-mail-phase2-pg-{tenant_a}")),
        )),
        hub: RealtimeHub::new(),
    });

    sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3), ($4, $5, $6)")
        .bind(tenant_a)
        .bind(format!("phase2-{tenant_a}"))
        .bind("Phase 2 parity tenant A")
        .bind(tenant_b)
        .bind(format!("phase2-{tenant_b}"))
        .bind("Phase 2 parity tenant B")
        .execute(&pool)
        .await
        .expect("insert isolated Postgres tenants");
    sqlx::query("INSERT INTO domains (id, tenant_id, domain) VALUES ($1, $2, $3), ($4, $5, $6)")
        .bind(domain_a)
        .bind(tenant_a)
        .bind(format!("phase2-{tenant_a}.test.local"))
        .bind(domain_b)
        .bind(tenant_b)
        .bind(format!("phase2-{tenant_b}.test.local"))
        .execute(&pool)
        .await
        .expect("insert isolated Postgres domains");
    sqlx::query("INSERT INTO mailboxes (id, tenant_id, domain_id, address) VALUES ($1, $2, $3, $4), ($5, $6, $7, $8)")
        .bind(mailbox_a)
        .bind(tenant_a)
        .bind(domain_a)
        .bind(format!("a-{tenant_a}@phase2.test.local"))
        .bind(mailbox_b)
        .bind(tenant_b)
        .bind(domain_b)
        .bind(format!("b-{tenant_b}@phase2.test.local"))
        .execute(&pool)
        .await
        .expect("insert isolated Postgres mailboxes");

    let app = api::router(state.clone());
    let admin = admin_token(&state, "admin@test.local");
    let user = admin_token(&state, "user@test.local");

    let result = async {
        let (status, issued) = admin_request(
            &app,
            Method::POST,
            "/v1/agent-access/grants",
            &admin,
            Some(serde_json::json!({
                "tenant_id": tenant_a,
                "mailbox_id": mailbox_a,
                "caller_id": "zeroclaw-mail-assistant",
                "audience": mcp_capabilities::MCP_AUDIENCE,
                "scopes": ["mail.search"],
                "expires_in_seconds": 900
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let raw_token = issued["access_token"]
            .as_str()
            .expect("Postgres HTTP issuance raw token")
            .to_string();
        assert_eq!(issued.to_string().matches(&raw_token).count(), 1);
        let http_grant_id = issued["grant"]["id"]
            .as_str()
            .expect("Postgres HTTP issuance grant id")
            .to_string();

        let (status, listed) =
            admin_request(&app, Method::GET, "/v1/agent-access/grants", &admin, None).await;
        assert_eq!(status, StatusCode::OK);
        let listed_text = listed.to_string();
        assert!(listed_text.contains(&http_grant_id));
        assert!(!listed_text.contains(&raw_token));
        assert!(!listed_text.contains("access_token"));

        let (status, _) = admin_request(
            &app,
            Method::DELETE,
            &format!("/v1/agent-access/grants/{http_grant_id}"),
            &admin,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            mcp_capabilities::validate_capability(
                &state,
                &raw_token,
                mcp_capabilities::MCP_AUDIENCE,
                None,
            )
            .await,
            Err(StatusCode::UNAUTHORIZED)
        );

        let (status, _) = admin_request(
            &app,
            Method::POST,
            "/v1/agent-access/grants",
            &user,
            Some(serde_json::json!({
                "tenant_id": tenant_a,
                "mailbox_id": mailbox_a,
                "caller_id": "zeroclaw-mail-assistant",
                "audience": mcp_capabilities::MCP_AUDIENCE,
                "scopes": ["mail.search"]
            })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (_, token) = mcp_capabilities::issue_capability(
            &state,
            mcp_capabilities::IssueCapabilityRequest {
                tenant_id: tenant_a.to_string(),
                mailbox_id: mailbox_a.to_string(),
                caller_id: "zeroclaw-mail-assistant".to_string(),
                audience: mcp_capabilities::MCP_AUDIENCE.to_string(),
                scopes: vec!["mail.search".to_string()],
                expires_in_seconds: Some(900),
            },
        )
        .await
        .expect("Postgres capability issuance");

        let context = mcp_capabilities::validate_capability(
            &state,
            &token,
            mcp_capabilities::MCP_AUDIENCE,
            Some("zeroclaw-mail-assistant"),
        )
        .await
        .expect("Postgres capability validation");
        assert_eq!(context.tenant_id, tenant_a);
        assert_eq!(context.mailbox_id, mailbox_a);
        assert_eq!(context.require_scope("mail.search"), Ok(()));
        assert_eq!(context.require_scope("mail.read"), Err(StatusCode::FORBIDDEN));

        let mismatch = mcp_capabilities::issue_capability(
            &state,
            mcp_capabilities::IssueCapabilityRequest {
                tenant_id: tenant_b.to_string(),
                mailbox_id: mailbox_a.to_string(),
                caller_id: "zeroclaw-mail-assistant".to_string(),
                audience: mcp_capabilities::MCP_AUDIENCE.to_string(),
                scopes: vec!["mail.search".to_string()],
                expires_in_seconds: None,
            },
        )
        .await;
        assert!(matches!(mismatch, Err(StatusCode::FORBIDDEN)));

        assert_eq!(
            mcp_capabilities::validate_capability(
                &state,
                &token,
                "wrong-audience",
                Some("zeroclaw-mail-assistant"),
            )
            .await,
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            mcp_capabilities::validate_capability(
                &state,
                &token,
                mcp_capabilities::MCP_AUDIENCE,
                Some("wrong-caller"),
            )
            .await,
            Err(StatusCode::FORBIDDEN)
        );

        sqlx::query("UPDATE mcp_capability_grants SET expires_at = NOW() - INTERVAL '1 second' WHERE token_hash = $1")
            .bind(token_hash(&token))
            .execute(&pool)
            .await
            .expect("expire isolated Postgres grant");
        assert_eq!(
            mcp_capabilities::validate_capability(
                &state,
                &token,
                mcp_capabilities::MCP_AUDIENCE,
                None,
            )
            .await,
            Err(StatusCode::UNAUTHORIZED)
        );

        let (_, revocable_token) = mcp_capabilities::issue_capability(
            &state,
            mcp_capabilities::IssueCapabilityRequest {
                tenant_id: tenant_a.to_string(),
                mailbox_id: mailbox_a.to_string(),
                caller_id: "zeroclaw-mail-assistant".to_string(),
                audience: mcp_capabilities::MCP_AUDIENCE.to_string(),
                scopes: vec!["mail.search".to_string()],
                expires_in_seconds: Some(900),
            },
        )
        .await
        .expect("Postgres revocable capability issuance");
        let grant_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM mcp_capability_grants WHERE token_hash = $1",
        )
        .bind(token_hash(&revocable_token))
        .fetch_one(&pool)
        .await
        .expect("find isolated Postgres grant");
        mcp_capabilities::revoke_capability(&state, grant_id)
            .await
            .expect("revoke Postgres capability");
        assert_eq!(
            mcp_capabilities::validate_capability(
                &state,
                &revocable_token,
                mcp_capabilities::MCP_AUDIENCE,
                None,
            )
            .await,
            Err(StatusCode::UNAUTHORIZED)
        );

        Ok::<(), String>(())
    }
    .await;

    sqlx::query("DELETE FROM mcp_capability_grants WHERE tenant_id IN ($1, $2)")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(&pool)
        .await
        .expect("cleanup isolated Postgres capability grants");
    sqlx::query("DELETE FROM mailboxes WHERE id IN ($1, $2)")
        .bind(mailbox_a)
        .bind(mailbox_b)
        .execute(&pool)
        .await
        .expect("cleanup isolated Postgres mailboxes");
    sqlx::query("DELETE FROM domains WHERE id IN ($1, $2)")
        .bind(domain_a)
        .bind(domain_b)
        .execute(&pool)
        .await
        .expect("cleanup isolated Postgres domains");
    sqlx::query("DELETE FROM tenants WHERE id IN ($1, $2)")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(&pool)
        .await
        .expect("cleanup isolated Postgres tenants");

    result.expect("Postgres capability parity assertions");
}

#[tokio::test]
async fn sqlite_send_confirmation_binds_payload_scope_and_is_one_time() {
    let fixture = fixture().await;
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.send"],
        None,
    )
    .await;
    let capability = mcp_capabilities::validate_capability(
        &fixture.state,
        &token,
        mcp_capabilities::MCP_AUDIENCE,
        None,
    )
    .await
    .expect("capability context");
    let context = aivory_mail_api::api::execution_context::ExecutionContext::from_capability(
        &capability,
        &axum::http::HeaderMap::new(),
    );
    let payload = SendRequest {
        from: "agent-a@test.local".to_string(),
        to: vec!["recipient@example.com".to_string()],
        cc: None,
        bcc: None,
        subject: "approved subject".to_string(),
        text: Some("approved body".to_string()),
        html: None,
        attachments: None,
        thread_id: None,
        in_reply_to: None,
    };
    let confirmation = mcp_confirmations::issue_send_confirmation(
        &fixture.state,
        mcp_confirmations::IssueSendConfirmationRequest {
            tenant_id: fixture.tenant_a.to_string(),
            mailbox_id: fixture.mailbox_a.to_string(),
            caller_id: capability.caller_id.clone(),
            payload: payload.clone(),
            expires_in_seconds: Some(60),
        },
    )
    .await
    .expect("confirmation issuance");
    assert_eq!(confirmation.action, mcp_confirmations::SEND_MAIL_ACTION);
    assert_eq!(confirmation.tenant_id, fixture.tenant_a.to_string());
    assert_eq!(confirmation.mailbox_id, fixture.mailbox_a.to_string());

    let mut changed = payload.clone();
    changed.subject = "mutated subject".to_string();
    assert_eq!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &context,
            &confirmation.id,
            &changed,
        )
        .await,
        Err(StatusCode::CONFLICT)
    );
    mcp_confirmations::consume_send_confirmation(
        &fixture.state,
        &context,
        &confirmation.id,
        &payload,
    )
    .await
    .expect("first consume");
    assert_eq!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &context,
            &confirmation.id,
            &payload,
        )
        .await,
        Err(StatusCode::CONFLICT)
    );
}

#[tokio::test]
async fn sqlite_v2_rejects_malformed_json_rpc_before_dispatch() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.search"],
        None,
    )
    .await;
    let (status, _) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({"jsonrpc":"1.0","id":1,"method":"tools/list"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sqlite_v2_knowledge_compilation_is_context_scoped_and_cached_by_pair() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.knowledge.read"],
        None,
    )
    .await;
    let (status, response) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":"get_knowledge_compile","arguments":{"budget":4000,"tenant_id":fixture.tenant_b,"mailbox_id":fixture.mailbox_b}}
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("knowledge text");
    assert!(text.contains(&fixture.tenant_a.to_string()));
    assert!(text.contains(&fixture.mailbox_a.to_string()));
    assert!(!text.contains(&fixture.tenant_b.to_string()));
    assert!(!text.contains(&fixture.mailbox_b.to_string()));
}

#[tokio::test]
async fn sqlite_v2_send_requires_confirmation_before_any_dispatch() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.send"],
        None,
    )
    .await;
    let (status, response) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":"send_mail","arguments":{"from":"agent-a@test.local","to":["recipient@example.com"],"subject":"not approved","text":"body"}}
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["error"]["code"], -32009);
    assert_eq!(response["error"]["message"], "send confirmation required before dispatch");
}

#[tokio::test]
async fn sqlite_v2_rejects_arguments_outside_centralized_bounds() {
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.search"],
        None,
    )
    .await;
    let (status, _) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "search_mail",
                "arguments": {"query": "invoice", "limit": 51}
            }
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sqlite_send_confirmation_rejects_expiry_binding_and_concurrent_replay() {
    let fixture = fixture().await;
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.send"],
        None,
    )
    .await;
    let capability = mcp_capabilities::validate_capability(
        &fixture.state,
        &token,
        mcp_capabilities::MCP_AUDIENCE,
        None,
    )
    .await
    .expect("capability context");
    let context = aivory_mail_api::api::execution_context::ExecutionContext::from_capability(
        &capability,
        &axum::http::HeaderMap::new(),
    );
    let payload = SendRequest {
        from: "agent-a@test.local".to_string(),
        to: vec!["recipient@example.com".to_string()],
        cc: None,
        bcc: None,
        subject: "approval binding".to_string(),
        text: Some("body".to_string()),
        html: None,
        attachments: None,
        thread_id: None,
        in_reply_to: None,
    };

    let confirmation = mcp_confirmations::issue_send_confirmation(
        &fixture.state,
        mcp_confirmations::IssueSendConfirmationRequest {
            tenant_id: fixture.tenant_a.to_string(),
            mailbox_id: fixture.mailbox_a.to_string(),
            caller_id: context.caller_id.clone(),
            payload: payload.clone(),
            expires_in_seconds: Some(60),
        },
    )
    .await
    .expect("confirmation issuance");
    let mut wrong_caller = context.clone();
    wrong_caller.caller_id = "different-agent".to_string();
    assert_eq!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &wrong_caller,
            &confirmation.id,
            &payload,
        )
        .await,
        Err(StatusCode::CONFLICT)
    );
    let mut wrong_mailbox = context.clone();
    wrong_mailbox.tenant_id = fixture.tenant_b;
    wrong_mailbox.mailbox_id = fixture.mailbox_b;
    assert_eq!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &wrong_mailbox,
            &confirmation.id,
            &payload,
        )
        .await,
        Err(StatusCode::CONFLICT)
    );

    let expired = mcp_confirmations::issue_send_confirmation(
        &fixture.state,
        mcp_confirmations::IssueSendConfirmationRequest {
            tenant_id: fixture.tenant_a.to_string(),
            mailbox_id: fixture.mailbox_a.to_string(),
            caller_id: context.caller_id.clone(),
            payload: payload.clone(),
            expires_in_seconds: Some(60),
        },
    )
    .await
    .expect("expired confirmation issuance");
    execute_sql(
        &fixture.state.db,
        &format!(
            "UPDATE mcp_send_confirmations SET expires_at='1970-01-01T00:00:00+00:00' WHERE id='{}'",
            expired.id
        ),
    )
    .await;
    assert_eq!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &context,
            &expired.id,
            &payload,
        )
        .await,
        Err(StatusCode::CONFLICT)
    );

    let concurrent = mcp_confirmations::issue_send_confirmation(
        &fixture.state,
        mcp_confirmations::IssueSendConfirmationRequest {
            tenant_id: fixture.tenant_a.to_string(),
            mailbox_id: fixture.mailbox_a.to_string(),
            caller_id: context.caller_id.clone(),
            payload: payload.clone(),
            expires_in_seconds: Some(60),
        },
    )
    .await
    .expect("concurrent confirmation issuance");
    let first_context = context.clone();
    let second_context = context.clone();
    let (first, second) = tokio::join!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &first_context,
            &concurrent.id,
            &payload,
        ),
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &second_context,
            &concurrent.id,
            &payload,
        )
    );
    assert!(matches!(
        (first, second),
        (Ok(()), Err(StatusCode::CONFLICT)) | (Err(StatusCode::CONFLICT), Ok(()))
    ));
}

/// Models a provider that has already accepted the message when a later,
/// unrelated step fails. In this fixture the outbound transport always takes
/// the dev "would send" path (no real provider configured, so send_email
/// treats it as a completed dispatch) and the messages table is deliberately
/// missing columns store_sent_message requires, forcing a failure strictly
/// *after* dispatch. The route must report the conservative unknown outcome,
/// not a definite success or failure, and must not allow the already-consumed
/// confirmation to trigger a second dispatch attempt.
#[tokio::test]
async fn sqlite_v2_send_reports_reconciliation_when_persistence_fails_after_dispatch() {
    // Integration tests never run main(), which is normally where this is
    // installed; the outbound HTTP client used by the transport fallback
    // chain needs it before its first request.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    std::env::set_var("AVRY_MCP_CAPABILITY_MODE", "v2");
    std::env::remove_var("RUST_ENV");
    std::env::remove_var("ENV");
    // Keep the transport fallback chain local and fast: never contact the
    // real worker route or MailChannels from a test.
    std::env::set_var("WORKER_SEND_URL", "http://127.0.0.1:1/send");
    std::env::set_var("MAILCHANNELS_DISABLE", "1");

    let fixture = fixture().await;
    let app = api::router(fixture.state.clone());
    let (_, token) = issue(
        &fixture,
        fixture.tenant_a,
        fixture.mailbox_a,
        &["mail.send"],
        None,
    )
    .await;
    let capability = mcp_capabilities::validate_capability(
        &fixture.state,
        &token,
        mcp_capabilities::MCP_AUDIENCE,
        None,
    )
    .await
    .expect("capability context");

    let payload = SendRequest {
        from: "agent-a@test.local".to_string(),
        to: vec!["recipient@example.com".to_string()],
        cc: None,
        bcc: None,
        subject: "reconciliation probe".to_string(),
        text: Some("body".to_string()),
        html: None,
        attachments: None,
        thread_id: None,
        in_reply_to: None,
    };
    let confirmation = mcp_confirmations::issue_send_confirmation(
        &fixture.state,
        mcp_confirmations::IssueSendConfirmationRequest {
            tenant_id: fixture.tenant_a.to_string(),
            mailbox_id: fixture.mailbox_a.to_string(),
            caller_id: capability.caller_id.clone(),
            payload: payload.clone(),
            expires_in_seconds: Some(60),
        },
    )
    .await
    .expect("confirmation issuance");

    let (status, response) = mcp_request(
        &app,
        Some(&token),
        serde_json::json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"tools/call",
            "params":{"name":"send_mail","arguments":{
                "from":"agent-a@test.local",
                "to":["recipient@example.com"],
                "subject":"reconciliation probe",
                "text":"body",
                "confirmation_id": confirmation.id
            }}
        }),
        &[],
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["error"]["code"], -32011);
    assert_eq!(
        response["error"]["message"],
        "send outcome is unknown; reconcile before retrying"
    );

    // The confirmation was already consumed before dispatch was attempted, so
    // the ambiguous outcome cannot be turned into an automatic or attacker
    // -driven retry: only a fresh, deliberately issued confirmation could
    // attempt the send again.
    let context = aivory_mail_api::api::execution_context::ExecutionContext::from_capability(
        &capability,
        &axum::http::HeaderMap::new(),
    );
    assert_eq!(
        mcp_confirmations::consume_send_confirmation(
            &fixture.state,
            &context,
            &confirmation.id,
            &payload,
        )
        .await,
        Err(StatusCode::CONFLICT)
    );

    std::env::remove_var("WORKER_SEND_URL");
    std::env::remove_var("MAILCHANNELS_DISABLE");
}
