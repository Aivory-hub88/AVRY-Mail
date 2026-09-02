use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use tracing::info;
use serde_json::Value;

#[derive(Clone)]
pub struct RealtimeHub {
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<Value>>>>,
}

impl RealtimeHub {
    pub fn new() -> Self {
        Self { channels: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn publish(&self, mailbox_id: &str, event: Value) {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(mailbox_id) {
            let _ = tx.send(event.clone());
            info!("realtime publish to {} {:?}", mailbox_id, event.get("type"));
        }
    }

    pub async fn subscribe(&self, mailbox_id: &str) -> broadcast::Receiver<Value> {
        let mut channels = self.channels.write().await;
        let tx = channels.entry(mailbox_id.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(100);
            tx
        }).clone();
        tx.subscribe()
    }

    pub async fn broadcast_new_message(&self, mailbox_id: &str, message: &serde_json::Value) {
        self.publish(mailbox_id, serde_json::json!({
            "type": "new_message",
            "mailbox_id": mailbox_id,
            "message": message,
            "ts": chrono::Utc::now().to_rfc3339()
        })).await;
    }

    pub async fn broadcast(&self, text: &str) {
        let msg: Value = serde_json::from_str(text).unwrap_or(serde_json::json!({"type":"broadcast","payload":text}));
        let channels = self.channels.read().await;
        for (_mb, tx) in channels.iter() {
            let _ = tx.send(msg.clone());
        }
        // also log
        tracing::info!("realtime broadcast {}", text);
    }
}

impl Default for RealtimeHub { fn default() -> Self { Self::new() } }
