use std::sync::Arc;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use crate::api::AppState;

/// GET /v1/cerveau/agents — list available Cerveau entrypoints that Mail can delegate to.
/// Does not require per-mailbox scope, but still requires auth so only logged-in Mail users see it.
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    // Require any authenticated Mail user (not just admin) — agents are visible to all.
    let _email = crate::api::authz::authenticated_email(&state, &headers)?;
    // Static list mirrors services/cerveau/skills/* entrypoints.
    // Hard-coded here so the API works even when the Cerveau daemon is down;
    // the relay below will proxy to the real daemon when it is up.
    let agents = vec![
        serde_json::json!({"id":"workflow_generate","skill":"workflow_generate","purpose":"LLM generates a workflow from a request"}),
        serde_json::json!({"id":"workflow_clarify","skill":"workflow_clarify","purpose":"Ask clarifying questions"}),
        serde_json::json!({"id":"workflow_edit","skill":"workflow_edit","purpose":"Edit an existing workflow"}),
        serde_json::json!({"id":"workflow_repair","skill":"workflow_repair","purpose":"Repair failed steps"}),
        serde_json::json!({"id":"workflow_semantic_review","skill":"workflow_semantic_review","purpose":"Blueprint semantic review — strict JSON findings"}),
        serde_json::json!({"id":"mail_ops","skill":"mail_ops","purpose":"Aivory Mail operator — per-mailbox inbox triage, Sent/delivery, thread memory (bridge via Mail MCP)"}),
        serde_json::json!({"id":"mail_memory","skill":"mail_memory","purpose":"Per-mailbox memory & knowledge compile — ingests /v1/cognee/sync"}),
    ];
    Ok(Json(serde_json::json!({"success": true, "data": agents, "via": state.config.cognee_url.clone().unwrap_or("heuristic".into())})))
}

/// POST /v1/cerveau/ask — Mail → Cerveau relay, per-mailbox isolated.
/// Body: { question: string, context?: {mailbox_id?, thread_id?, message_id?}, agent?: string, history?: [] }
/// The Mail AI assistant (zeroclaw vanilla) calls this when it needs deep memory:
/// Cerveau will receive the mailbox_id and can call back into Mail via MCP (search_mail etc.)
/// with the same mailbox_id — so the whole Chain-of-Thought stays scoped to one mailbox.
pub async fn relay(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    // 1. Auth — derive own mailbox, enforce ownership like ai_chat::ask.
    // Admin without a mailbox (admin@aivory.id) has no own_mid row — allow them
    // to relay for any mailbox_id they explicitly provide.
    let email_for_admin_check = crate::api::authz::authenticated_email(&state, &headers)?;
    let is_admin = crate::api::authz::is_admin(&state, &email_for_admin_check).await;
    let own_res = crate::api::ai_chat::own_mailbox_id_for_relay(&state, &headers).await;
    let (own_mid, own_email) = match own_res {
        Ok(v) => v,
        Err(StatusCode::NOT_FOUND) if is_admin => {
            // admin has no mailbox row — use their email as identity, require explicit mailbox_id
            let ctx_mid = body
                .get("context")
                .and_then(|c| c.get("mailbox_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if ctx_mid.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            (ctx_mid.clone(), email_for_admin_check.clone())
        }
        Err(e) => return Err(e),
    };

    let question = body.get("question").or_else(|| body.get("q")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if question.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let ctx = body.get("context").cloned().unwrap_or(Value::Null);
    let mut mailbox_id = ctx.get("mailbox_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if mailbox_id.is_empty() {
        mailbox_id = own_mid.clone();
    } else if mailbox_id != own_mid && !is_admin {
        return Err(StatusCode::FORBIDDEN);
    }
    let agent = body.get("agent").and_then(|v| v.as_str()).unwrap_or("mail_ops").to_string();
    let thread_id = ctx.get("thread_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let message_id = ctx.get("message_id").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // 2. Build a Cerveau-friendly payload — include mailbox-scoped context that Cerveau can use
    // to call back via MCP with the same mailbox_id (so MCP's search is scoped).
    let payload = serde_json::json!({
        "agent": agent,
        "entrypoint": agent,
        "question": question,
        "mailbox_id": mailbox_id,
        "mailbox_email": own_email,
        "context": {
            "mailbox_id": mailbox_id,
            "thread_id": thread_id,
            "message_id": message_id,
            "history": body.get("history").cloned().unwrap_or(Value::Array(vec![]))
        },
        "mcp": {
            "base": state.config.public_base_url,
            "tools": ["search_mail","get_inbox_overview","get_thread_memory","get_knowledge_compile","send_mail"],
            "hint": "Call Mail MCP at POST /mcp with x-internal-token or Bearer apiKey, pass mailbox_id to stay scoped"
        },
        "callback": {
            "mail_api": state.config.public_base_url,
            "auth": "Bearer <user_jwt> or x-internal-token"
        }
    });

    // 3. Try Cerveau / Cognee URL first (the daemon that hosts skills).
    // Preference: COGNEE_URL (Cerveau/Cognee-RS) → AI_GATEWAY_URL (zeroclaw vanilla) → heuristic.
    let mut answer: Option<Value> = None;
    let mut via = "heuristic";

    // Try COGNEE_URL as Cerveau daemon (POST /v1/cerveau/ask or /invoke) — ignore empty string.
    if let Some(cog_url) = &state.config.cognee_url {
        if cog_url.trim().is_empty() {
            // no-op — fall through to AI gateway / heuristic
        } else {
            let urls = [format!("{}/v1/cerveau/ask", cog_url.trim_end_matches('/')), format!("{}/invoke", cog_url.trim_end_matches('/'))];
        for url in &urls {
            let mut req = reqwest::Client::new()
                .post(url)
                .header("content-type", "application/json")
                .header("x-mailbox-id", &mailbox_id)
                .header("x-mailbox-email", &own_email)
                .json(&payload)
                .timeout(std::time::Duration::from_secs(10));
            if let Some(sec) = &state.config.cognee_secret {
                req = req.header("x-cerveau-internal-secret", sec).header("x-internal-token", &state.config.internal_token);
            } else {
                req = req.header("x-internal-token", &state.config.internal_token);
            }
            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    if let Ok(j) = resp.json::<Value>().await {
                        // Normalize: Cerveau may return {answer} or {data:{answer}} or {choices:[...]}
                        let ans = j.get("answer").cloned()
                            .or_else(|| j.get("data").and_then(|d| d.get("answer")).cloned())
                            .or_else(|| j.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first()).and_then(|v| v.get("message")).and_then(|m| m.get("content")).cloned());
                        if let Some(a) = ans {
                            answer = Some(serde_json::json!({"answer": a, "raw": j}));
                            via = "cerveau";
                            break;
                        } else if j.is_object() {
                            answer = Some(j);
                            via = "cerveau";
                            break;
                        }
                    }
                }
            }
        }
        }
    }

    // Fallback to AI_GATEWAY_URL (zeroclaw vanilla) — same payload but via its /v1/ai/chat
    if answer.is_none() {
        if let Some(ai_url) = &state.config.ai_gateway_url {
            // Ask zeroclaw as if it were a Cerveau mail_ops agent — include mailbox context in system prompt.
            let prompt = serde_json::json!([
                {"role":"system","content": format!("You are Cerveau agent '{}' for Mail. Answering for mailbox {} ({}). Use MCP tools with mailbox_id={} to stay scoped.", agent, mailbox_id, own_email, mailbox_id)},
                {"role":"system","content": format!("MCP available: search_mail, get_inbox_overview, get_thread_memory (mailbox_id={}), get_knowledge_compile, send_mail. Call via POST /mcp.", mailbox_id)},
                {"role":"user","content": question}
            ]);
            if let Ok(resp) = reqwest::Client::new()
                .post(format!("{}/v1/ai/chat", ai_url.trim_end_matches('/')))
                .header("x-internal-token", &state.config.internal_token)
                .header("x-mailbox-id", &mailbox_id)
                .json(&serde_json::json!({"model": state.config.mail_intelligence_model, "messages": prompt, "temperature": 0.3}))
                .timeout(std::time::Duration::from_secs(8))
                .send().await
            {
                if let Ok(j) = resp.json::<Value>().await {
                    let c = j.get("choices").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.get("message")).and_then(|m| m.get("content")).and_then(|v| v.as_str());
                    if let Some(content) = c {
                        answer = Some(serde_json::json!({"answer": content, "raw": j}));
                        via = "zeroclaw";
                    }
                }
            }
        }
    }

    // Heuristic fallback
    let final_val = answer.unwrap_or_else(|| {
        via = "heuristic";
        serde_json::json!({"answer": aivory_mail_core::email_assistant::heuristic_fallback(&question, "", ""), "via": "heuristic"})
    });

    // Optionally save a trace in ai_chat_history as a Cerveau relay (not as normal ask)
    let _ = crate::api::ai_chat::save_cerveau_relay(&state.db, &mailbox_id, &own_email, &question, &final_val, &agent).await;

    Ok(Json(serde_json::json!({"success": true, "via": via, "data": final_val, "context": {"mailbox_id": mailbox_id, "agent": agent}})))
}
