use std::sync::Arc;

use axum::extract::{Path, State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::Response;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::routes::sessions::parse_id;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum WsMessage {
    SessionUpdated(lgtm_session::Session),
    DiffUpdated(Vec<lgtm_git::DiffFile>),
    SubmitStatus(SubmitStatusData),
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct SubmitStatusData {
    pub pending: bool,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<crate::AppState>>,
    Path(session_id): Path<String>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<crate::AppState>, session_id: String) {
    let id = match parse_id(&session_id) {
        Ok(id) => id,
        Err(_) => {
            let _ = socket.send(Message::Text("{\"error\":\"invalid session id\"}".into())).await;
            return;
        }
    };

    let rx = {
        let channels = state.broadcast_channels.read().unwrap();
        channels.get(&id).map(|tx| tx.subscribe())
    };
    let Some(rx) = rx else {
        let _ = socket.send(Message::Text("{\"error\":\"session not found\"}".into())).await;
        return;
    };

    // Send initial session state
    if let Ok(session) = state.store.get(id) {
        let msg = WsMessage::SessionUpdated(session);
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }

    // Forward broadcast messages to this client
    forward_messages(socket, rx).await;
}

async fn forward_messages(mut socket: WebSocket, mut rx: broadcast::Receiver<WsMessage>) {
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
