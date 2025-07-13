//! Basic example demonstrating the WebSocket builder framework
//!
//! This example shows how to create a simple WebSocket server with middleware.
//! The same code works with both tungstenite and fastwebsockets backends.
//!
//! Run the example:
//! ```
//! cargo run --example basic_demo
//! ```

use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::ConnectInfo, response::IntoResponse, routing::get, Router};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    InboundContext, MessageConverterTrait, Middleware, SendMessage, WebSocketBuilder,
};

// Simple string converter
#[derive(Clone)]
struct StringConverter;

impl MessageConverterTrait<String, String> for StringConverter {
    fn inbound_from_bytes(&self, bytes: &[u8]) -> Result<Option<String>> {
        if bytes.is_empty() {
            return Ok(None);
        }
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(Some(s.to_string())),
            Err(e) => Err(anyhow::anyhow!("Invalid UTF-8: {}", e)),
        }
    }

    fn outbound_to_string(&self, message: String) -> Result<String> {
        Ok(message)
    }
}

// Per-connection state that tracks connection-specific information
#[derive(Debug, Clone)]
struct ConnectionState {
    connection_id: String,
    message_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            connection_id: String::new(),
            message_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

// No longer need StateFactory - state is created directly

// Use the built-in StringConverter

// Logging middleware that works with both backends
#[derive(Debug)]
struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    type State = ConnectionState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            let count = ctx
                .state
                .read()
                .message_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            println!(
                "[LoggingMiddleware] Connection {} received message #{}: {}",
                ctx.state.read().connection_id,
                count,
                msg
            );
        }
        ctx.next().await
    }
}

// Echo middleware that prefixes messages
#[derive(Debug)]
struct PrefixEchoMiddleware {
    prefix: String,
}

#[async_trait]
impl Middleware for PrefixEchoMiddleware {
    type State = ConnectionState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(message) = &ctx.message {
            let response = format!("{}: {}", self.prefix, message);
            ctx.send_message(response)?;
        }
        ctx.next().await
    }
}

// WebSocket handler using the new unified API
// This works with both tungstenite and fastwebsockets!
async fn ws_handler(
    ws: websocket_builder::WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    handler: Arc<WebSocketHandler>,
) -> impl IntoResponse {
    use websocket_builder::UnifiedWebSocketExt;

    let connection_id = addr.to_string(); // Use actual IP:port as connection ID
    let cancellation_token = CancellationToken::new();

    println!("New WebSocket connection from: {connection_id}");

    // Create state for this connection
    let state = ConnectionState {
        connection_id: connection_id.clone(),
        message_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };

    handler
        .handle_upgrade(ws, connection_id, cancellation_token, state)
        .await
}

// Type alias for the handler to make it cleaner
type WebSocketHandler =
    websocket_builder::WebSocketHandler<ConnectionState, String, String, StringConverter>;

#[tokio::main]
async fn main() -> Result<()> {
    // Build the WebSocket handler with middleware
    let builder = WebSocketBuilder::new(StringConverter)
        .with_middleware(LoggingMiddleware)
        .with_middleware(PrefixEchoMiddleware {
            prefix: "ECHO".to_string(),
        });

    let handler = Arc::new(builder.build());

    // Build the router - this code is identical regardless of backend
    let app = Router::new().route(
        "/ws",
        get({
            let handler = handler.clone();
            move |ws, addr| ws_handler(ws, addr, handler)
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();

    println!("Server running on http://127.0.0.1:3001");
    println!("WebSocket endpoint: ws://127.0.0.1:3001/ws");
    println!();
    println!("Test with: websocat ws://127.0.0.1:3001/ws");
    println!("The server will echo messages with 'ECHO: ' prefix");

    #[cfg(feature = "tungstenite")]
    println!("Using tungstenite backend");
    #[cfg(feature = "fastwebsockets")]
    println!("Using fastwebsockets backend");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();

    Ok(())
}
