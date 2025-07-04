//! Pipeline demo showing middleware composition
//!
//! This example demonstrates how to build a middleware pipeline with multiple
//! stages that process messages in sequence.

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    extract::{ConnectInfo, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    ConnectionContext, DisconnectContext, InboundContext, Middleware, OutboundContext, SendMessage,
    StateFactory, StringConverter, UnifiedWebSocketExt, WebSocketBuilder, WebSocketUpgrade,
};

// Application state with connection counter
#[derive(Debug)]
struct AppState {
    total_connections: AtomicU64,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
        }
    }
}

// State factory
#[derive(Clone)]
struct AppStateFactory;

impl StateFactory<Arc<AppState>> for AppStateFactory {
    fn create_state(&self, _token: CancellationToken) -> Arc<AppState> {
        Arc::new(AppState::default())
    }
}

// Use the built-in StringConverter

// Middleware 1: Connection tracking
#[derive(Debug)]
struct ConnectionTracker;

#[async_trait]
impl Middleware for ConnectionTracker {
    type State = Arc<AppState>;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn on_connect(
        &self,
        ctx: &mut ConnectionContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        let count = {
            let state = ctx.state.read();
            state.total_connections.fetch_add(1, Ordering::Relaxed)
        };
        println!(
            "[ConnectionTracker] Connection established. Total: {}",
            count + 1
        );
        ctx.send_message(format!("Welcome! You are connection #{}", count + 1))?;
        ctx.next().await
    }

    async fn on_disconnect(
        &self,
        ctx: &mut DisconnectContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        println!("[ConnectionTracker] Connection closed");
        ctx.next().await
    }
}

// Middleware 2: Logging
#[derive(Debug)]
struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    type State = Arc<AppState>;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            println!("[LoggingMiddleware] Received: {msg}");
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        println!("[LoggingMiddleware] Sending: {:?}", ctx.message);
        ctx.next().await
    }
}

// Middleware 3: Echo with transformation
#[derive(Debug)]
struct TransformEchoMiddleware;

#[async_trait]
impl Middleware for TransformEchoMiddleware {
    type State = Arc<AppState>;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(message) = &ctx.message {
            let response = format!("Echo: {}", message.to_uppercase());
            ctx.send_message(response)?;
        }
        ctx.next().await
    }
}

// Middleware 4: Message stats
#[derive(Debug, Default)]
struct StatsMiddleware {
    message_count: AtomicU64,
}

#[async_trait]
impl Middleware for StatsMiddleware {
    type State = Arc<AppState>;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        let count = self.message_count.fetch_add(1, Ordering::Relaxed) + 1;

        if let Some(message) = &ctx.message {
            if message == "stats" {
                ctx.send_message(format!("Total messages processed: {count}"))?;
                return Ok(()); // Don't process further
            }
        }

        ctx.next().await
    }
}

// WebSocket handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    State(handler): State<Arc<WebSocketHandler>>,
) -> impl IntoResponse {
    let connection_id = addr.to_string(); // Use actual IP:port
    let cancellation_token = CancellationToken::new();

    println!("New WebSocket connection from: {connection_id}");

    handler
        .handle_upgrade(ws, connection_id, cancellation_token)
        .await
}

type WebSocketHandler = websocket_builder::WebSocketHandler<
    Arc<AppState>,
    String,
    String,
    StringConverter,
    AppStateFactory,
>;

#[tokio::main]
async fn main() -> Result<()> {
    // Build the middleware pipeline
    let builder = WebSocketBuilder::new(AppStateFactory, StringConverter::new())
        .with_middleware(ConnectionTracker)
        .with_middleware(LoggingMiddleware)
        .with_middleware(TransformEchoMiddleware)
        .with_middleware(StatsMiddleware::default());

    let handler = Arc::new(builder.build());

    // Create the Axum app
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(handler);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002")
        .await
        .unwrap();

    println!("\n📡 Server running on http://127.0.0.1:3002");
    println!("🔌 WebSocket endpoint: ws://127.0.0.1:3002/ws");
    println!("\n📝 Middleware pipeline:");
    println!("   1. ConnectionTracker - Tracks connections");
    println!("   2. LoggingMiddleware - Logs all messages");
    println!("   3. TransformEchoMiddleware - Echoes messages in uppercase");
    println!("   4. StatsMiddleware - Responds to 'stats' command");
    println!("\n🧪 Test with: websocat ws://127.0.0.1:3002/ws");
    println!("   - Send any message to see it echoed in uppercase");
    println!("   - Send 'stats' to see message count");
    println!("   - Watch the console for middleware logs\n");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .unwrap();

    Ok(())
}
