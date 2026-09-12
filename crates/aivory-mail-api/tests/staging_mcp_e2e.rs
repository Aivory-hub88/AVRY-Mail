use anyhow::{anyhow, Context, Result};
use reqwest::{redirect::Policy, Client, Response, StatusCode, Url};
use serde_json::{json, Value};
use std::{env, net::IpAddr, time::Duration};
use uuid::Uuid;

const MCP_AUDIENCE: &str = "aivory-mail-mcp";

struct Config {
    api_url: String,
    admin_email: String,
    admin_password: String,
    tenant_id: String,
    mailbox_id: String,
    foreign_tenant_id: String,
    foreign_mailbox_id: String,
    agent_id: String,
    own_sentinel: String,
    foreign_sentinel: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        let api_url = required("STAGING_API_URL")?;
        let parsed_url = Url::parse(&api_url).context("STAGING_API_URL must be a valid URL")?;
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(anyhow!("STAGING_API_URL must use http or https"));
        }
        let host = parsed_url
            .host_str()
            .ok_or_else(|| anyhow!("STAGING_API_URL must contain a host"))?;
        let allowed_host = required("STAGING_ALLOWED_HOST")?;
        if host != allowed_host {
            return Err(anyhow!(
                "STAGING_API_URL host does not match STAGING_ALLOWED_HOST"
            ));
        }
        let normalized_host = host.trim_end_matches('.').to_ascii_lowercase();
        if normalized_host == "mail.aivory.uk"
            || normalized_host == "mail.aivory.id"
            || normalized_host.ends_with(".aivory.uk")
            || normalized_host.ends_with(".aivory.id")
        {
            return Err(anyhow!(
                "production Aivory host is not allowed for staging E2E"
            ));
        }
        let is_loopback = normalized_host == "localhost"
            || normalized_host
                .parse::<IpAddr>()
                .map(|address| address.is_loopback())
                .unwrap_or(false);
        if !is_loopback && env::var("STAGING_ALLOW_REMOTE").as_deref() != Ok("1") {
            return Err(anyhow!(
                "non-loopback staging host requires STAGING_ALLOW_REMOTE=1"
            ));
        }

        let tenant_id = required("STAGING_TENANT_ID")?;
        let mailbox_id = required("STAGING_MAILBOX_ID")?;
        let foreign_tenant_id = required("STAGING_FOREIGN_TENANT_ID")?;
        let foreign_mailbox_id = required("STAGING_FOREIGN_MAILBOX_ID")?;

        for (name, value) in [
            ("STAGING_TENANT_ID", &tenant_id),
            ("STAGING_MAILBOX_ID", &mailbox_id),
            ("STAGING_FOREIGN_TENANT_ID", &foreign_tenant_id),
            ("STAGING_FOREIGN_MAILBOX_ID", &foreign_mailbox_id),
        ] {
            Uuid::parse_str(value).with_context(|| format!("{name} must be a UUID"))?;
        }

        Ok(Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            admin_email: required("STAGING_ADMIN_EMAIL")?,
            admin_password: required("STAGING_ADMIN_PASSWORD")?,
            tenant_id,
            mailbox_id,
            foreign_tenant_id,
            foreign_mailbox_id,
            agent_id: required("STAGING_AGENT_ID")?,
            own_sentinel: required("STAGING_OWN_SENTINEL")?,
            foreign_sentinel: required("STAGING_FOREIGN_SENTINEL")?,
        })
    }
}

fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} must be set for the staging E2E test"))
}

#[tokio::test]
#[ignore = "requires explicit STAGING_E2E=1 and an isolated staging service"]
async fn staging_mcp_capability_e2e() {
    if env::var("STAGING_E2E").as_deref() != Ok("1") {
        eprintln!("skipping staging E2E: set STAGING_E2E=1 to opt in");
        return;
    }

    if let Err(error) = run().await {
        panic!("staging MCP E2E failed: {error:#}");
    }
}

async fn run() -> Result<()> {
    let config = Config::from_env()?;
    let client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .context("build staging E2E HTTP client")?;

    let admin_token = login(&client, &config).await?;
    let caller_id = format!("{}-e2e-{}", config.agent_id, Uuid::new_v4());

    let assertions = execute_flow(&client, &config, &admin_token, &caller_id).await;
    let cleanup = cleanup_grants(&client, &config, &admin_token, &caller_id).await;

    match (assertions, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(assertion_error), Ok(())) => Err(assertion_error),
        (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
        (Err(assertion_error), Err(cleanup_error)) => Err(anyhow!(
            "{assertion_error:#}; cleanup also failed: {cleanup_error:#}"
        )),
    }
}

async fn execute_flow(
    client: &Client,
    config: &Config,
    admin_token: &str,
    caller_id: &str,
) -> Result<()> {
    let health = client
        .get(endpoint(config, "/health"))
        .send()
        .await
        .context("request staging health")?;
    let health = expect_status(health, StatusCode::OK, "staging health").await?;
    let health_json: Value = health.json().await.context("decode staging health")?;
    if health_json.get("status") != Some(&Value::String("ok".to_string())) {
        return Err(anyhow!("staging health did not report status=ok"));
    }

    let (_grant_a, token_a) = issue_grant(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.read", "mail.search"]),
        600,
    )
    .await?;

    let (status, tools) = mcp_request(
        client,
        config,
        Some(&token_a),
        Some(caller_id),
        "/mcp",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::OK, "valid tools/list")?;
    let tools_result = rpc_result(&tools, 1, "valid tools/list")?;
    if !tools_result["tools"]
        .as_array()
        .map(|tools| tools.iter().any(|tool| tool["name"] == "search_mail"))
        .unwrap_or(false)
    {
        return Err(anyhow!(
            "valid tools/list response did not contain search_mail"
        ));
    }

    let (status, own_search) = mcp_request(
        client,
        config,
        Some(&token_a),
        Some(caller_id),
        "/mcp",
        json!({
            "jsonrpc":"2.0",
            "id":2,
            "method":"tools/call",
            "params":{"name":"search_mail","arguments":{"query":config.own_sentinel,"limit":10,"mailbox_id":config.foreign_mailbox_id,"tenant_id":config.foreign_tenant_id}}
        }),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::OK, "mailbox A search")?;
    let own_results = rpc_text_value(&own_search, 2, "mailbox A search")?;
    if !own_results.is_array()
        || !own_results.as_array().is_some_and(|results| {
            results.iter().any(|result| {
                result["subject"]
                    .as_str()
                    .is_some_and(|subject| subject.contains(&config.own_sentinel))
            })
        })
    {
        return Err(anyhow!("mailbox A search did not return its sentinel"));
    }

    let (status, cross_search) = mcp_request(
        client,
        config,
        Some(&token_a),
        Some(caller_id),
        "/mcp",
        json!({
            "jsonrpc":"2.0",
            "id":3,
            "method":"tools/call",
            "params":{"name":"search_mail","arguments":{"query":config.foreign_sentinel,"limit":10}}
        }),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::OK, "cross-mailbox search")?;
    let foreign_results = rpc_text_value(&cross_search, 3, "cross-mailbox search")?;
    if !foreign_results
        .as_array()
        .is_some_and(|results| results.is_empty())
    {
        return Err(anyhow!(
            "mailbox A cross-mailbox search did not return an empty result"
        ));
    }

    let (status, overview_response) = mcp_request(
        client,
        config,
        Some(&token_a),
        Some(caller_id),
        "/mcp",
        json!({
            "jsonrpc":"2.0",
            "id":4,
            "method":"tools/call",
            "params":{"name":"get_inbox_overview","arguments":{}}
        }),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::OK, "mailbox A read call")?;
    let overview = rpc_text_value(&overview_response, 4, "mailbox A read call")?;
    if !overview["total"].is_number() || !overview["unread_inbox"].is_number() {
        return Err(anyhow!("mailbox A overview returned an invalid result"));
    }

    for (id, tool_name, arguments) in [
        (
            13,
            "get_thread_memory",
            json!({"thread_id":Uuid::new_v4().to_string()}),
        ),
        (14, "get_knowledge_compile", json!({})),
        (
            15,
            "send_mail",
            json!({"to":["sink@example.invalid"],"subject":"blocked","text":"blocked"}),
        ),
    ] {
        let (status, _) = mcp_request(
            client,
            config,
            Some(&token_a),
            Some(caller_id),
            "/mcp",
            json!({
                "jsonrpc":"2.0",
                "id":id,
                "method":"tools/call",
                "params":{"name":tool_name,"arguments":arguments}
            }),
            &[],
        )
        .await?;
        ensure_status(status, StatusCode::FORBIDDEN, "read-only tool boundary")?;
    }

    let (_, search_only_token) = issue_grant(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.search"]),
        600,
    )
    .await?;
    let (status, _) = mcp_request(
        client,
        config,
        Some(&search_only_token),
        Some(caller_id),
        "/mcp",
        json!({
            "jsonrpc":"2.0",
            "id":5,
            "method":"tools/call",
            "params":{"name":"get_inbox_overview","arguments":{}}
        }),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::FORBIDDEN, "insufficient search scope")?;

    let (_, read_only_token) = issue_grant(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.read"]),
        600,
    )
    .await?;
    let (status, _) = mcp_request(
        client,
        config,
        Some(&read_only_token),
        Some(caller_id),
        "/mcp",
        json!({
            "jsonrpc":"2.0",
            "id":12,
            "method":"tools/call",
            "params":{"name":"search_mail","arguments":{"query":config.own_sentinel}}
        }),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::FORBIDDEN, "insufficient read scope")?;

    let invalid_scope_status = issue_status(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        "aivory-mail-mcp",
        json!(["mail.admin"]),
        600,
    )
    .await?;
    ensure_status(
        invalid_scope_status,
        StatusCode::BAD_REQUEST,
        "invalid scope",
    )?;

    let wrong_audience_status = issue_status(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        "wrong-audience",
        json!(["mail.read"]),
        600,
    )
    .await?;
    ensure_status(
        wrong_audience_status,
        StatusCode::BAD_REQUEST,
        "wrong audience",
    )?;

    let foreign_tenant_mismatch_status = issue_status(
        client,
        config,
        admin_token,
        &config.foreign_tenant_id,
        &config.mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.read"]),
        600,
    )
    .await?;
    ensure_status(
        foreign_tenant_mismatch_status,
        StatusCode::FORBIDDEN,
        "foreign tenant/mailbox mismatch",
    )?;

    let mismatch_status = issue_status(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.foreign_mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.read"]),
        600,
    )
    .await?;
    ensure_status(
        mismatch_status,
        StatusCode::FORBIDDEN,
        "tenant/mailbox mismatch",
    )?;

    let (status, _) = mcp_request(
        client,
        config,
        Some(&token_a),
        Some("wrong-caller"),
        "/mcp",
        json!({"jsonrpc":"2.0","id":6,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_rejected(status, "wrong caller")?;

    let (status, no_caller_tools) = mcp_request(
        client,
        config,
        Some(&token_a),
        None,
        "/mcp",
        json!({"jsonrpc":"2.0","id":16,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::OK, "capability without caller header")?;
    rpc_result(&no_caller_tools, 16, "capability without caller header")?;

    let (status, _) = mcp_request(
        client,
        config,
        Some("avry_mcp_unknown_staging_token"),
        Some(caller_id),
        "/mcp",
        json!({"jsonrpc":"2.0","id":17,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::UNAUTHORIZED, "unknown bearer token")?;

    let (status, _) = mcp_request(
        client,
        config,
        None,
        None,
        "/mcp",
        json!({"jsonrpc":"2.0","id":7,"method":"initialize"}),
        &[(
            "x-internal-token",
            "staging-internal-token-must-not-authorize-mcp",
        )],
    )
    .await?;
    ensure_status(status, StatusCode::UNAUTHORIZED, "internal token bypass")?;

    let (status, _) = mcp_request(
        client,
        config,
        None,
        None,
        "/mcp",
        json!({"jsonrpc":"2.0","id":18,"method":"initialize"}),
        &[(
            "x-cerveau-internal-secret",
            "staging-cognee-secret-must-not-authorize-mcp",
        )],
    )
    .await?;
    ensure_status(status, StatusCode::UNAUTHORIZED, "Cerveau secret bypass")?;

    let (status, _) = mcp_request(
        client,
        config,
        None,
        None,
        "/mcp?api_key=legacy-global-key",
        json!({"jsonrpc":"2.0","id":8,"method":"initialize"}),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::UNAUTHORIZED, "legacy query-key bypass")?;

    let (status, _) = mcp_request(
        client,
        config,
        Some(admin_token),
        Some(caller_id),
        "/mcp",
        json!({"jsonrpc":"2.0","id":9,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_rejected(status, "admin JWT as MCP capability")?;

    let (_, expiring_token) = issue_grant(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.read"]),
        1,
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (status, _) = mcp_request(
        client,
        config,
        Some(&expiring_token),
        Some(caller_id),
        "/mcp",
        json!({"jsonrpc":"2.0","id":10,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::UNAUTHORIZED, "expired capability")?;

    let (revocable_grant, revocable_token) = issue_grant(
        client,
        config,
        admin_token,
        &config.tenant_id,
        &config.mailbox_id,
        caller_id,
        MCP_AUDIENCE,
        json!(["mail.read"]),
        600,
    )
    .await?;
    let revoke_response = client
        .delete(endpoint(
            config,
            &format!("/v1/agent-access/grants/{revocable_grant}"),
        ))
        .bearer_auth(admin_token)
        .send()
        .await
        .context("revoke staging capability")?;
    let revoke_response =
        expect_status(revoke_response, StatusCode::OK, "revoke staging capability").await?;
    let _: Value = revoke_response
        .json()
        .await
        .context("decode revoke response")?;
    let (status, _) = mcp_request(
        client,
        config,
        Some(&revocable_token),
        Some(caller_id),
        "/mcp",
        json!({"jsonrpc":"2.0","id":11,"method":"tools/list"}),
        &[],
    )
    .await?;
    ensure_status(status, StatusCode::UNAUTHORIZED, "revoked capability")?;

    Ok(())
}

async fn login(client: &Client, config: &Config) -> Result<String> {
    let response = client
        .post(endpoint(config, "/v1/auth/login"))
        .json(&json!({
            "email": config.admin_email,
            "password": config.admin_password
        }))
        .send()
        .await
        .context("staging admin login request")?;
    let response = expect_status(response, StatusCode::OK, "staging admin login").await?;
    let value: Value = response.json().await.context("decode staging login")?;
    if value.get("success") != Some(&Value::Bool(true)) {
        return Err(anyhow!("staging admin login returned success=false"));
    }
    value["data"]["token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("staging admin login did not return a token"))
}

async fn issue_grant(
    client: &Client,
    config: &Config,
    admin_token: &str,
    tenant_id: &str,
    mailbox_id: &str,
    caller_id: &str,
    audience: &str,
    scopes: Value,
    expires_in_seconds: i64,
) -> Result<(String, String)> {
    let response = client
        .post(endpoint(config, "/v1/agent-access/grants"))
        .bearer_auth(admin_token)
        .json(&grant_body(
            tenant_id,
            mailbox_id,
            caller_id,
            audience,
            scopes,
            expires_in_seconds,
        ))
        .send()
        .await
        .context("issue staging capability")?;
    let response = expect_status(response, StatusCode::CREATED, "issue staging capability").await?;
    let value: Value = response
        .json()
        .await
        .context("decode capability issuance")?;
    let grant_id = value["grant"]["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("capability issuance did not return a grant id"))?;
    let token = value["access_token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("capability issuance did not return an access token"))?;
    Ok((grant_id, token))
}

async fn issue_status(
    client: &Client,
    config: &Config,
    admin_token: &str,
    tenant_id: &str,
    mailbox_id: &str,
    caller_id: &str,
    audience: &str,
    scopes: Value,
    expires_in_seconds: i64,
) -> Result<StatusCode> {
    let response = client
        .post(endpoint(config, "/v1/agent-access/grants"))
        .bearer_auth(admin_token)
        .json(&grant_body(
            tenant_id,
            mailbox_id,
            caller_id,
            audience,
            scopes,
            expires_in_seconds,
        ))
        .send()
        .await
        .context("issue negative staging capability case")?;
    Ok(response.status())
}

fn grant_body(
    tenant_id: &str,
    mailbox_id: &str,
    caller_id: &str,
    audience: &str,
    scopes: Value,
    expires_in_seconds: i64,
) -> Value {
    json!({
        "tenant_id": tenant_id,
        "mailbox_id": mailbox_id,
        "caller_id": caller_id,
        "audience": audience,
        "scopes": scopes,
        "expires_in_seconds": expires_in_seconds
    })
}

async fn mcp_request(
    client: &Client,
    config: &Config,
    token: Option<&str>,
    caller_id: Option<&str>,
    path: &str,
    body: Value,
    extra_headers: &[(&str, &str)],
) -> Result<(StatusCode, Value)> {
    let mut request = client
        .post(endpoint(config, path))
        .header("content-type", "application/json")
        .json(&body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(caller_id) = caller_id {
        request = request.header("x-agent-id", caller_id);
    }
    for (name, value) in extra_headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.context("MCP staging request")?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .context("read MCP staging response")?;
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Ok((status, value))
}

async fn cleanup_grants(
    client: &Client,
    config: &Config,
    admin_token: &str,
    caller_id: &str,
) -> Result<()> {
    let mut errors = Vec::new();
    let grants = match list_grants(client, config, admin_token).await {
        Ok(grants) => grants,
        Err(first_error) => match list_grants(client, config, admin_token).await {
            Ok(grants) => grants,
            Err(retry_error) => {
                return Err(anyhow!(
                    "staging grant cleanup could not list grants: {first_error:#}; retry failed: {retry_error:#}"
                ));
            }
        },
    };

    let grant_ids: Vec<String> = grants
        .iter()
        .filter(|grant| grant["caller_id"].as_str() == Some(caller_id))
        .filter_map(|grant| grant["id"].as_str().map(ToString::to_string))
        .collect();

    for grant_id in grant_ids {
        let result = client
            .delete(endpoint(
                config,
                &format!("/v1/agent-access/grants/{grant_id}"),
            ))
            .bearer_auth(admin_token)
            .send()
            .await;
        match result {
            Ok(response) if response.status() == StatusCode::OK => {}
            Ok(response) => errors.push(format!(
                "delete staging E2E grant returned HTTP {}",
                response.status()
            )),
            Err(error) => errors.push(format!("delete staging E2E grant failed: {error}")),
        }
    }

    match list_grants(client, config, admin_token).await {
        Ok(grants) => {
            let remaining = grants
                .iter()
                .filter(|grant| grant["caller_id"].as_str() == Some(caller_id))
                .count();
            if remaining != 0 {
                errors.push(format!("staging E2E cleanup left {remaining} grants"));
            }
        }
        Err(error) => errors.push(format!("verify staging grant cleanup failed: {error:#}")),
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("{}", errors.join("; ")))
    }
}

async fn list_grants(client: &Client, config: &Config, admin_token: &str) -> Result<Vec<Value>> {
    let response = client
        .get(endpoint(config, "/v1/agent-access/grants"))
        .bearer_auth(admin_token)
        .send()
        .await
        .context("list staging grants")?;
    let response = expect_status(response, StatusCode::OK, "list staging grants").await?;
    let value: Value = response.json().await.context("decode staging grant list")?;
    value["grants"]
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow!("staging grant list did not contain grants"))
}

async fn expect_status(
    response: Response,
    expected: StatusCode,
    operation: &str,
) -> Result<Response> {
    let actual = response.status();
    if actual != expected {
        return Err(anyhow!(
            "{operation} returned HTTP {actual}, expected {expected}"
        ));
    }
    Ok(response)
}

fn ensure_status(actual: StatusCode, expected: StatusCode, operation: &str) -> Result<()> {
    if actual != expected {
        return Err(anyhow!(
            "{operation} returned HTTP {actual}, expected {expected}"
        ));
    }
    Ok(())
}

fn ensure_rejected(actual: StatusCode, operation: &str) -> Result<()> {
    if actual != StatusCode::UNAUTHORIZED && actual != StatusCode::FORBIDDEN {
        return Err(anyhow!(
            "{operation} returned HTTP {actual}, expected 401 or 403"
        ));
    }
    Ok(())
}

fn rpc_result<'a>(value: &'a Value, id: i64, operation: &str) -> Result<&'a Value> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || value.get("id").and_then(Value::as_i64) != Some(id)
    {
        return Err(anyhow!("{operation} returned an invalid JSON-RPC envelope"));
    }
    if value.get("error").is_some() {
        return Err(anyhow!("{operation} returned a JSON-RPC error"));
    }
    value
        .get("result")
        .ok_or_else(|| anyhow!("{operation} did not return a JSON-RPC result"))
}

fn rpc_text_value(value: &Value, id: i64, operation: &str) -> Result<Value> {
    let result = rpc_result(value, id, operation)?;
    let text = result["content"]
        .as_array()
        .and_then(|content| content.first())
        .and_then(|item| item["text"].as_str())
        .ok_or_else(|| anyhow!("{operation} did not return MCP text content"))?;
    serde_json::from_str(text)
        .with_context(|| format!("{operation} returned invalid MCP text JSON"))
}

fn endpoint(config: &Config, path: &str) -> String {
    format!(
        "{}{}",
        config.api_url,
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        }
    )
}
