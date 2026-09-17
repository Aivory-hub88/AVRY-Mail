use std::sync::Arc;
use axum::{extract::{State, ws::{WebSocketUpgrade, WebSocket, Message}, Query}, http::StatusCode, response::IntoResponse};
use serde_json::Value;
use sqlx::Row;
use crate::api::AppState;

/// Browser WebSocket clients cannot send an Authorization header, so the
/// session JWT travels as ?token=. The token is verified here and the
/// mailbox_id is scoped to the token owner's own mailbox (admins may pass
/// any). Without this the socket is unusable from browsers and the inbox
/// never updates without a manual refresh.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<Value>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
    let token = params.get("token").and_then(|v| v.as_str()).unwrap_or("");
    let claims = crate::auth::verify_jwt(token, &state.config.jwt_secret).map_err(|_| StatusCode::UNAUTHORIZED)?;
    if claims.role.as_deref() == Some("oauth-state") {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let email = claims.sub.trim().to_lowercase();
    if email.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let requested = params.get("mailbox_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if requested.is_empty() || requested == "global" {
        return Err(StatusCode::UNAUTHORIZED);
    }
    // Resolve the caller's own mailbox id.
    let own_id: Option<String> = match &state.db {
        aivory_mail_storage::db::DbPool::Postgres(pool) => {
            sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=lower($1) LIMIT 1")
                .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| r.get::<uuid::Uuid,_>("id").to_string())
        }
        aivory_mail_storage::db::DbPool::Sqlite(pool) => {
            sqlx::query("SELECT id FROM mailboxes WHERE lower(address)=lower(?) LIMIT 1")
                .bind(&email).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(|r| r.get::<String,_>("id"))
        }
    };
    let admin = crate::api::authz::is_admin(&state, &email).await;
    let allowed = admin || own_id.as_deref() == Some(requested.as_str());
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, requested)))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, mailbox_id: String) {
    let mut rx = state.hub.subscribe(&mailbox_id).await;
    // send hello
    let _ = socket.send(Message::Text(serde_json::json!({"type":"connected","mailbox_id": mailbox_id}).to_string().into())).await;
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(val) => {
                        if socket.send(Message::Text(val.to_string().into())).await.is_err() { break; }
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        // ping/pong
                        if t.contains("ping") {
                            let _ = socket.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
