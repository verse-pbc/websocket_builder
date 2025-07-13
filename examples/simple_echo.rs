//! Simplest possible WebSocket echo server
//!
//! This example shows the most concise way to use websocket_builder.
//! It works identically with both tungstenite and fastwebsockets backends.
//!
//! Run with tungstenite (default):
//! ```
//! cargo run --example simple_echo
//! ```
//!
//! Run with fastwebsockets:
//! ```
//! cargo run --example simple_echo --no-default-features --features fastwebsockets
//! ```

use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::ConnectInfo, routing::get, Router};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    InboundContext, MessageConverterTrait, Middleware, SendMessage, WebSocketBuilder,
};

// Per-connection state (empty for echo server)
#[derive(Debug, Clone, Default)]
struct ConnectionState;

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

// Simple echo middleware
#[derive(Debug)]
struct EchoMiddleware;

#[async_trait]
impl Middleware for EchoMiddleware {
    type State = ConnectionState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(message) = &ctx.message {
            ctx.send_message(message.clone())?;
        }
        ctx.next().await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Build the handler
    let handler = Arc::new(
        WebSocketBuilder::new(StringConverter)
            .with_middleware(EchoMiddleware)
            .build(),
    );

    // Create the app - simple handler function
    async fn ws_handler(
        ws: websocket_builder::WebSocketUpgrade,
        ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
        handler: Arc<
            websocket_builder::WebSocketHandler<ConnectionState, String, String, StringConverter>,
        >,
    ) -> axum::response::Response {
        use websocket_builder::UnifiedWebSocketExt;
        let connection_id = addr.to_string(); // Use actual IP:port
        let cancellation_token = CancellationToken::new();
        println!("Echo connection from: {connection_id}");
        let state = ConnectionState;
        handler
            .handle_upgrade(ws, connection_id, cancellation_token, state)
            .await
    }

    let app = Router::new().route(
        "/ws",
        get({
            let handler = handler.clone();
            move |ws, addr| ws_handler(ws, addr, handler)
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002").await?;

    println!("Echo server running on ws://127.0.0.1:3002/ws");
    println!("Test with: websocat ws://127.0.0.1:3002/ws");

    #[cfg(feature = "tungstenite")]
    println!("Backend: tungstenite");
    #[cfg(feature = "fastwebsockets")]
    println!("Backend: fastwebsockets");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
