use std::sync::Arc;
use axum::{extract::{State, ws::{WebSocketUpgrade, WebSocket, Message}, Query}, response::IntoResponse};
use serde_json::Value;
use crate::api::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<Value>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let mailbox_id = params.get("mailbox_id").and_then(|v| v.as_str()).unwrap_or("global").to_string();
    ws.on_upgrade(move |socket| handle_socket(socket, state, mailbox_id))
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
