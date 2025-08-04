//! WebSocket connection handling
//!
//! This module contains the core logic for managing WebSocket connections.

use crate::handler::{DisconnectReason, WebSocketHandler};
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use std::net::SocketAddr;
use tracing::{debug, error, trace};

/// Configuration for WebSocket connections
#[derive(Debug, Clone, Default)]
pub struct ConnectionConfig {
    /// Maximum number of concurrent connections (None for unlimited)
    pub max_connections: Option<usize>,
}

/// Handle a WebSocket connection
///
/// This function manages the lifecycle of a WebSocket connection:
/// 1. Calls `on_connect` when the connection is established
/// 2. Routes incoming text messages to `on_message`
/// 3. Handles ping/pong automatically (via axum)
/// 4. Calls `on_disconnect` when the connection closes
pub async fn handle_socket<H: WebSocketHandler>(
    socket: WebSocket,
    addr: SocketAddr,
    mut handler: H,
    _config: ConnectionConfig,
) {
    debug!("New WebSocket connection from {}", addr);

    let (ws_sink, mut ws_stream) = socket.split();

    // Call on_connect with the WebSocket sink directly
    if let Err(e) = handler.on_connect(addr, ws_sink).await {
        error!("Handler on_connect error: {}", e);
        return;
    }

    // Process incoming messages
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                trace!("Received text message: {} bytes", text.len());
                if let Err(e) = handler.on_message(text).await {
                    error!("Handler on_message error: {}", e);
                    break;
                }
            }
            Ok(Message::Binary(_)) => {
                trace!("Ignoring binary message");
                // Ignore binary messages as per design
            }
            Ok(Message::Close(frame)) => {
                debug!("Received close frame: {:?}", frame);
                let reason = DisconnectReason::from_close_frame(frame);
                handler.on_disconnect(reason).await;
                break;
            }
            Ok(Message::Ping(_) | Message::Pong(_)) => {
                // Axum handles ping/pong automatically
                trace!("Received ping/pong");
            }
            Err(e) => {
                // Check if this is a connection reset error
                // This is a common occurrence when clients disconnect abruptly
                let error_str = e.to_string();

                if error_str.contains("Connection reset") {
                    debug!("WebSocket connection reset: {}", e);
                } else {
                    error!("WebSocket error: {}", e);
                }
                handler
                    .on_disconnect(DisconnectReason::Error(error_str))
                    .await;
                break;
            }
        }
    }

    debug!("WebSocket connection from {} closed", addr);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();
        assert_eq!(config.max_connections, None);
    }

    #[test]
    fn test_connection_config_with_limit() {
        let config = ConnectionConfig {
            max_connections: Some(100),
        };
        assert_eq!(config.max_connections, Some(100));
    }

    #[test]
    fn test_connection_config_clone() {
        let config = ConnectionConfig {
            max_connections: Some(50),
        };
        let cloned = config.clone();
        assert_eq!(cloned.max_connections, config.max_connections);
    }
}
