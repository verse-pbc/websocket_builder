//! Middleware pipeline demonstration
//!
//! This example shows how multiple middleware components work together in sequence.
//! Each middleware can modify messages, add logging, perform validation, etc.
//!
//! Run with:
//! ```
//! cargo run --example pipeline_demo
//! ```

use anyhow::Result;
use async_trait::async_trait;
use axum::{extract::ConnectInfo, routing::get, Router};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    InboundContext, MessageConverterTrait, Middleware, OutboundContext, SendMessage,
    WebSocketBuilder,
};

// Per-connection state that tracks metrics
#[derive(Debug, Clone)]
struct ConnectionState {
    messages_received: Arc<std::sync::atomic::AtomicUsize>,
    messages_sent: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            messages_received: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            messages_sent: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

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

// 1. Logging middleware - logs all messages
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
                .messages_received
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            println!("[LoggingMiddleware] Inbound message #{count}: {msg}");
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            let count = ctx
                .state
                .read()
                .messages_sent
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            println!("[LoggingMiddleware] Outbound message #{count}: {msg}");
        }
        ctx.next().await
    }

    async fn on_connect(
        &self,
        _ctx: &mut ConnectionContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        println!("[LoggingMiddleware] Client connected");
        Ok(())
    }

    async fn on_disconnect(
        &self,
        _ctx: &mut DisconnectContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        println!("[LoggingMiddleware] Client disconnected");
        Ok(())
    }
}

// 2. Validation middleware - validates message format
#[derive(Debug)]
struct ValidationMiddleware;

#[async_trait]
impl Middleware for ValidationMiddleware {
    type State = ConnectionState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            // Example validation: reject messages that are too long
            if msg.len() > 1000 {
                println!(
                    "[ValidationMiddleware] Rejecting message - too long ({} bytes)",
                    msg.len()
                );
                ctx.send_message("Error: Message too long (max 1000 bytes)".to_string())?;
                return Ok(()); // Don't continue to next middleware
            }

            // Example validation: reject messages with prohibited content
            if msg.contains("spam") {
                println!("[ValidationMiddleware] Rejecting message - contains prohibited content");
                ctx.send_message("Error: Message contains prohibited content".to_string())?;
                return Ok(()); // Don't continue to next middleware
            }

            println!("[ValidationMiddleware] Message passed validation");
        }
        ctx.next().await
    }
}

// 3. Transform middleware - transforms messages
#[derive(Debug)]
struct TransformMiddleware;

#[async_trait]
impl Middleware for TransformMiddleware {
    type State = ConnectionState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &mut ctx.message {
            // Transform: convert to uppercase
            *msg = msg.to_uppercase();
            println!("[TransformMiddleware] Transformed message to uppercase");
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &mut ctx.message {
            // Add a simple prefix to outbound messages
            *msg = format!("[SERVER] {msg}");
            println!("[TransformMiddleware] Added server prefix to outbound message");
        }
        ctx.next().await
    }
}

// 4. Echo middleware - the actual business logic
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
            println!("[EchoMiddleware] Echoing transformed message: {message}");
            ctx.send_message(format!("ECHO: {message}"))?;
        }
        ctx.next().await
    }
}

use websocket_builder::{ConnectionContext, DisconnectContext};

// WebSocket handler
async fn ws_handler(
    ws: websocket_builder::WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    handler: Arc<
        websocket_builder::WebSocketHandler<ConnectionState, String, String, StringConverter>,
    >,
) -> axum::response::Response {
    use websocket_builder::UnifiedWebSocketExt;
    let connection_id = addr.to_string();
    let cancellation_token = CancellationToken::new();
    println!("\nNew connection from: {connection_id}");
    let state = ConnectionState::default();
    handler
        .handle_upgrade(ws, connection_id, cancellation_token, state)
        .await
}

#[tokio::main]
async fn main() -> Result<()> {
    // Build the handler with a pipeline of middleware
    let handler = Arc::new(
        WebSocketBuilder::new(StringConverter)
            .with_middleware(LoggingMiddleware) // First: log everything
            .with_middleware(ValidationMiddleware) // Second: validate messages
            .with_middleware(TransformMiddleware) // Third: transform messages
            .with_middleware(EchoMiddleware) // Finally: echo back
            .build(),
    );

    let app = Router::new().route(
        "/ws",
        get({
            let handler = handler.clone();
            move |ws, addr| ws_handler(ws, addr, handler)
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3004").await?;

    println!("Pipeline demo server running on ws://127.0.0.1:3004/ws");
    println!("\nMiddleware pipeline order:");
    println!("1. LoggingMiddleware    - Logs all messages");
    println!("2. ValidationMiddleware - Validates message format");
    println!("3. TransformMiddleware  - Converts to uppercase, adds server prefix");
    println!("4. EchoMiddleware       - Echoes the transformed message");
    println!("\nTest with: websocat ws://127.0.0.1:3004/ws");
    println!("Try sending:");
    println!("  - 'hello world' (will be transformed to uppercase)");
    println!("  - 'spam' (will be rejected by validation)");
    println!("  - A very long message (>1000 chars, will be rejected)");

    #[cfg(feature = "tungstenite")]
    println!("\nBackend: tungstenite");
    #[cfg(feature = "fastwebsockets")]
    println!("\nBackend: fastwebsockets");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
