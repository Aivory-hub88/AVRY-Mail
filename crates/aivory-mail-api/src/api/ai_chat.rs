use std::sync::Arc;
use axum::{extract::{State, Query}, Json, http::StatusCode};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;
use crate::api::AppState;
use aivory_mail_storage::db::DbPool;

/// POST /v1/ai/ask  — Ask AI Assistant (zeroclaw vanilla)
/// body: { question: string, context?: {mailbox_id?, message_id?, thread_id?}, history?: [] }
pub async fn ask(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let question = body.get("question").or_else(|| body.get("q")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if question.is_empty() { return Err(StatusCode::BAD_REQUEST); }
    let ctx = body.get("context").cloned().unwrap_or(Value::Null);
    let mailbox_id = ctx.get("mailbox_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let message_id = ctx.get("message_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let thread_id = ctx.get("thread_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let user_email = body.get("user_email").and_then(|v| v.as_str()).unwrap_or("anon@aivory.uk").to_string();

    // 1. Gather context: selected message + thread + inbox overview
    let (subject, body_text, snippet) = if !message_id.is_empty() {
        fetch_message_context(&state.db, &message_id).await.unwrap_or(("".into(),"".into(),"".into()))
    } else if !thread_id.is_empty() {
        fetch_thread_context(&state.db, &thread_id).await.unwrap_or(("".into(),"".into(),"".into()))
    } else { ("".into(),"".into(),"".into()) };

    let heuristic = aivory_mail_core::intelligence::analyze(&subject, &body_text);
    let inbox_overview = fetch_overview(&state.db).await;
    let thread_memory = if !thread_id.is_empty() { fetch_thread_memory(&state.db, &thread_id, 2000).await } else { None };
    let context_summary = if !subject.is_empty() || !snippet.is_empty() { format!("subject: {} | snippet: {} | heuristic: {}/{}", subject, snippet, heuristic.intent, format!("{:?}", heuristic.urgency)) } else { "".into() };

    // 2. Try zeroclaw vanilla AI_GATEWAY_URL first
    let mut answer: Option<String> = None;
    let mut model_used = "heuristic".to_string();
    let prompt_msgs = aivory_mail_core::email_assistant::build_prompt(&question, &context_summary, thread_memory.as_deref(), Some(&inbox_overview));

    if let Some(ai_url) = &state.config.ai_gateway_url {
        if let Ok(resp) = reqwest::Client::new()
            .post(format!("{}/v1/ai/chat", ai_url))
            .header("x-internal-token", &state.config.internal_token)
            .json(&serde_json::json!({"model": state.config.mail_intelligence_model, "messages": prompt_msgs, "temperature": 0.3}))
            .timeout(std::time::Duration::from_secs(8))
            .send().await
        {
            if let Ok(j) = resp.json::<Value>().await {
                if let Some(c) = j.get("choices").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.get("message")).and_then(|m| m.get("content")).and_then(|v| v.as_str()) {
                    answer = Some(c.to_string());
                    model_used = state.config.mail_intelligence_model.clone();
                } else if let Some(c) = j.get("answer").or_else(|| j.get("content")).and_then(|v| v.as_str()) {
                    answer = Some(c.to_string());
                    model_used = "zeroclaw".into();
                } else if let Some(c) = j.get("data").and_then(|v| v.get("answer")).and_then(|v| v.as_str()) {
                    answer = Some(c.to_string());
                    model_used = "zeroclaw".into();
                }
            }
        }
    }

    // 3. Fallback OpenRouter direct
    if answer.is_none() {
        if let Some(or_key) = std::env::var("OPENROUTER_API_KEY").ok().filter(|s| !s.is_empty()) {
            // prefer config openrouter key fallback: state.config holds it indirectly via env
            let or_key = if or_key.starts_with("sk-or-") { or_key } else { std::env::var("OPENROUTER_API_KEY").unwrap_or_default() };
            let payload = serde_json::json!({
                "model": state.config.mail_intelligence_model,
                "messages": prompt_msgs,
                "temperature": 0.3,
                "max_tokens": 800
            });
            if let Ok(resp) = reqwest::Client::new()
                .post("https://openrouter.ai/api/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", or_key))
                .header("HTTP-Referer", "https://mail.aivory.uk")
                .header("X-Title", "Aivory Mail Email Assistant")
                .json(&payload)
                .timeout(std::time::Duration::from_secs(10))
                .send().await
            {
                if let Ok(j) = resp.json::<Value>().await {
                    if let Some(c) = j.get("choices").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.get("message")).and_then(|m| m.get("content")).and_then(|v| v.as_str()) {
                        answer = Some(c.to_string());
                        model_used = state.config.mail_intelligence_model.clone();
                    }
                }
            }
        }
    }

    // 4. Fallback heuristic
    let final_answer = answer.unwrap_or_else(|| aivory_mail_core::email_assistant::heuristic_fallback(&question, &subject, &body_text));

    // 5. Save history
    let _ = save_chat(&state.db, &mailbox_id, &user_email, &question, &final_answer, &ctx, &model_used).await;

    // 6. Auto push suggestion if High urgency
    let should_push = matches!(heuristic.urgency, aivory_mail_core::types::Urgency::High) && (heuristic.intent=="invoice" || heuristic.intent=="meeting_request");
    let suggested_actions = aivory_mail_core::intelligence::suggest_actions(&heuristic.intent, &heuristic.urgency);

    Ok(Json(serde_json::json!({
        "success": true,
        "data": {
            "answer": final_answer,
            "model": model_used,
            "sources": {
                "message_id": if message_id.is_empty() { Value::Null } else { Value::String(message_id.clone()) },
                "thread_id": if thread_id.is_empty() { Value::Null } else { Value::String(thread_id.clone()) },
                "heuristic": heuristic,
                "inbox_overview": inbox_overview
            },
            "suggested_actions": suggested_actions,
            "auto_push_suggested": should_push,
            "thread_memory": thread_memory.unwrap_or_default()
        }
    })))
}

pub async fn history(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let mailbox_id = q.get("mailbox_id").and_then(|v| v.as_str()).unwrap_or("");
    let limit: i64 = q.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).min(100);
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if mailbox_id.is_empty() {
                sqlx::query("SELECT id, user_email, question, answer, model, created_at FROM ai_chat_history ORDER BY created_at DESC LIMIT $1")
                    .bind(limit).fetch_all(pool).await.unwrap_or_default()
            } else {
                sqlx::query("SELECT id, user_email, question, answer, model, created_at FROM ai_chat_history WHERE mailbox_id=$1 ORDER BY created_at DESC LIMIT $2")
                    .bind(mailbox_id).bind(limit).fetch_all(pool).await.unwrap_or_default()
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<Uuid,_>("id").to_string(),
                "user_email": row.get::<String,_>("user_email"),
                "question": row.get::<String,_>("question"),
                "answer": row.get::<String,_>("answer"),
                "model": row.get::<String,_>("model"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if mailbox_id.is_empty() {
                sqlx::query("SELECT id, user_email, question, answer, model, created_at FROM ai_chat_history ORDER BY created_at DESC LIMIT ?")
                    .bind(limit).fetch_all(pool).await.unwrap_or_default()
            } else {
                sqlx::query("SELECT id, user_email, question, answer, model, created_at FROM ai_chat_history WHERE mailbox_id=? ORDER BY created_at DESC LIMIT ?")
                    .bind(mailbox_id).bind(limit).fetch_all(pool).await.unwrap_or_default()
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "user_email": row.get::<String,_>("user_email"),
                "question": row.get::<String,_>("question"),
                "answer": row.get::<String,_>("answer"),
                "model": row.get::<String,_>("model"),
                "created_at": row.get::<String,_>("created_at")
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

/// POST /v1/ai/push-to-mission-control — push notification ke Mission Control
pub async fn push_to_mission_control(State(state): State<Arc<AppState>>, Json(body): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("Email Assistant").to_string();
    let bdy = body.get("body").or_else(|| body.get("message")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let typ = body.get("type").and_then(|v| v.as_str()).unwrap_or("email_assistant").to_string();
    let action_url = body.get("action_url").or_else(|| body.get("url")).and_then(|v| v.as_str()).unwrap_or("https://mail.aivory.uk").to_string();
    let metadata = body.get("metadata").cloned().unwrap_or(serde_json::json!({}));
    let id = Uuid::new_v4();
    let now = chrono::Utc::now().to_rfc3339();

    // Insert
    let ok = match &state.db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO mission_control_notifications (id, type, title, body, action_url, metadata_json, is_read, created_at) VALUES ($1,$2,$3,$4,$5,$6,false,$7)")
                .bind(id).bind(&typ).bind(&title).bind(&bdy).bind(&action_url).bind(&metadata).bind(chrono::Utc::now())
                .execute(pool).await.is_ok()
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO mission_control_notifications (id, type, title, body, action_url, metadata_json, is_read, created_at) VALUES (?,?,?,?,?,?,0,?)")
                .bind(id.to_string()).bind(&typ).bind(&title).bind(&bdy).bind(&action_url).bind(serde_json::to_string(&metadata).unwrap_or("{}".into())).bind(&now)
                .execute(pool).await.is_ok()
        }
    };

    // Broadcast via RealtimeHub
    state.hub.broadcast(&serde_json::json!({
        "type": "mission_control_notification",
        "id": id.to_string(),
        "title": title,
        "body": bdy,
        "action_url": action_url
    }).to_string()).await;

    // Forward to WORKFLOW_URL (n8n) if configured — best-effort
    if let Some(wf) = &state.config.workflow_url {
        let payload = serde_json::json!({"type": typ, "title": title, "body": bdy, "action_url": action_url, "metadata": metadata, "id": id.to_string()});
        let _ = reqwest::Client::new().post(format!("{}/webhook/email-assistant-notify", wf))
            .json(&payload).timeout(std::time::Duration::from_secs(5)).send().await;
        // also try backend dashboard webhook if reachable
        let _ = reqwest::Client::new().post(format!("{}/webhook/email-assistant", wf))
            .json(&payload).timeout(std::time::Duration::from_secs(5)).send().await;
    }

    if ok {
        Ok(Json(serde_json::json!({"success": true, "data": {"id": id.to_string(), "title": title}})))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// GET /v1/notifications — polled by Mission Control widget (dashboard.aivory.id)
pub async fn list_notifications(State(state): State<Arc<AppState>>, Query(q): Query<Value>) -> Result<Json<Value>, StatusCode> {
    let limit: i64 = q.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).min(100);
    let typ = q.get("type").and_then(|v| v.as_str());
    let rows: Vec<Value> = match &state.db {
        DbPool::Postgres(pool) => {
            let r = if let Some(t) = typ {
                sqlx::query("SELECT id, type, title, body, action_url, metadata_json, is_read, created_at FROM mission_control_notifications WHERE type=$1 ORDER BY created_at DESC LIMIT $2")
                    .bind(t).bind(limit).fetch_all(pool).await.unwrap_or_default()
            } else {
                sqlx::query("SELECT id, type, title, body, action_url, metadata_json, is_read, created_at FROM mission_control_notifications ORDER BY created_at DESC LIMIT $1")
                    .bind(limit).fetch_all(pool).await.unwrap_or_default()
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<Uuid,_>("id").to_string(),
                "type": row.get::<String,_>("type"),
                "title": row.get::<String,_>("title"),
                "body": row.get::<String,_>("body"),
                "action_url": row.get::<Option<String>,_>("action_url"),
                "metadata": row.get::<Option<Value>,_>("metadata_json").unwrap_or(Value::Null),
                "is_read": row.get::<bool,_>("is_read"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at").to_rfc3339()
            })).collect()
        }
        DbPool::Sqlite(pool) => {
            let r = if let Some(t) = typ {
                sqlx::query("SELECT id, type, title, body, action_url, metadata_json, is_read, created_at FROM mission_control_notifications WHERE type=? ORDER BY created_at DESC LIMIT ?")
                    .bind(t).bind(limit).fetch_all(pool).await.unwrap_or_default()
            } else {
                sqlx::query("SELECT id, type, title, body, action_url, metadata_json, is_read, created_at FROM mission_control_notifications ORDER BY created_at DESC LIMIT ?")
                    .bind(limit).fetch_all(pool).await.unwrap_or_default()
            };
            r.into_iter().map(|row| serde_json::json!({
                "id": row.get::<String,_>("id"),
                "type": row.get::<String,_>("type"),
                "title": row.get::<String,_>("title"),
                "body": row.get::<String,_>("body"),
                "action_url": row.get::<Option<String>,_>("action_url"),
                "metadata": serde_json::from_str::<Value>(&row.get::<String,_>("metadata_json")).unwrap_or(Value::Null),
                "is_read": row.get::<i32,_>("is_read")!=0,
                "created_at": row.get::<String,_>("created_at")
            })).collect()
        }
    };
    Ok(Json(serde_json::json!({"success": true, "data": rows})))
}

// helpers
async fn fetch_message_context(db: &DbPool, mid: &str) -> Option<(String,String,String)> {
    match db {
        DbPool::Postgres(pool) => {
            let uid = Uuid::parse_str(mid).ok()?;
            let row = sqlx::query("SELECT subject, body_text, snippet FROM messages WHERE id=$1").bind(uid).fetch_optional(pool).await.ok()??;
            Some((row.get::<Option<String>,_>("subject").unwrap_or_default(), row.get::<Option<String>,_>("body_text").unwrap_or_default(), row.get::<Option<String>,_>("snippet").unwrap_or_default()))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT subject, body_text, snippet FROM messages WHERE id=?").bind(mid).fetch_optional(pool).await.ok()??;
            Some((row.get::<Option<String>,_>("subject").unwrap_or_default(), row.get::<Option<String>,_>("body_text").unwrap_or_default(), row.get::<Option<String>,_>("snippet").unwrap_or_default()))
        }
    }
}
async fn fetch_thread_context(db: &DbPool, tid: &str) -> Option<(String,String,String)> {
    match db {
        DbPool::Postgres(pool) => {
            let uid = Uuid::parse_str(tid).ok()?;
            let row = sqlx::query("SELECT subject FROM threads WHERE id=$1").bind(uid).fetch_optional(pool).await.ok()??;
            let subj = row.get::<Option<String>,_>("subject").unwrap_or_default();
            let msg = sqlx::query("SELECT body_text, snippet FROM messages WHERE thread_id=$1 ORDER BY created_at DESC LIMIT 1").bind(uid).fetch_optional(pool).await.ok()??;
            Some((subj, msg.get::<Option<String>,_>("body_text").unwrap_or_default(), msg.get::<Option<String>,_>("snippet").unwrap_or_default()))
        }
        DbPool::Sqlite(pool) => {
            let row = sqlx::query("SELECT subject FROM threads WHERE id=?").bind(tid).fetch_optional(pool).await.ok()??;
            let subj = row.get::<Option<String>,_>("subject").unwrap_or_default();
            let msg = sqlx::query("SELECT body_text, snippet FROM messages WHERE thread_id=? ORDER BY created_at DESC LIMIT 1").bind(tid).fetch_all(pool).await.ok()?;
            let m = msg.first()?;
            Some((subj, m.get::<Option<String>,_>("body_text").unwrap_or_default(), m.get::<Option<String>,_>("snippet").unwrap_or_default()))
        }
    }
}
async fn fetch_overview(db: &DbPool) -> String {
    match db {
        DbPool::Postgres(pool) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
            let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=false").fetch_one(pool).await.unwrap_or(0);
            format!("total {}, unread_inbox {}", total, unread)
        }
        DbPool::Sqlite(pool) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(pool).await.unwrap_or(0);
            let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE folder='Inbox' AND is_read=0").fetch_one(pool).await.unwrap_or(0);
            format!("total {}, unread_inbox {}", total, unread)
        }
    }
}
async fn fetch_thread_memory(db: &DbPool, tid: &str, budget: usize) -> Option<String> {
    match db {
        DbPool::Postgres(pool) => {
            let uid = Uuid::parse_str(tid).ok()?;
            let rows = sqlx::query("SELECT subject, snippet FROM messages WHERE thread_id=$1 ORDER BY created_at DESC LIMIT 5").bind(uid).fetch_all(pool).await.ok()?;
            let mut out = String::new();
            let mut used=0;
            for r in rows { let s: String = format!("{} | {} \n", r.get::<Option<String>,_>("subject").unwrap_or_default(), r.get::<Option<String>,_>("snippet").unwrap_or_default()); if used + s.len() > budget { break; } used+=s.len(); out.push_str(&s); }
            Some(out)
        }
        DbPool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT subject, snippet FROM messages WHERE thread_id=? ORDER BY created_at DESC LIMIT 5").bind(tid).fetch_all(pool).await.ok()?;
            let mut out = String::new();
            let mut used=0;
            for r in rows { let s: String = format!("{} | {} \n", r.get::<Option<String>,_>("subject").unwrap_or_default(), r.get::<Option<String>,_>("snippet").unwrap_or_default()); if used + s.len() > budget { break; } used+=s.len(); out.push_str(&s); }
            Some(out)
        }
    }
}
async fn save_chat(db: &DbPool, mailbox_id: &str, user_email: &str, q: &str, ans: &str, ctx: &Value, model: &str) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    match db {
        DbPool::Postgres(pool) => {
            sqlx::query("INSERT INTO ai_chat_history (id, mailbox_id, user_email, question, answer, context_json, model, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(id).bind(if mailbox_id.is_empty(){ None } else { Some(mailbox_id)}).bind(user_email).bind(q).bind(ans).bind(ctx).bind(model).bind(now)
                .execute(pool).await?;
        }
        DbPool::Sqlite(pool) => {
            sqlx::query("INSERT INTO ai_chat_history (id, mailbox_id, user_email, question, answer, context_json, model, created_at) VALUES (?,?,?,?,?,?,?,?)")
                .bind(id.to_string()).bind(mailbox_id).bind(user_email).bind(q).bind(ans).bind(serde_json::to_string(ctx).unwrap_or("{}".into())).bind(model).bind(now.to_rfc3339())
                .execute(pool).await?;
        }
    }
    Ok(())
}
