# WebSocket Builder

[![Crates.io](https://img.shields.io/crates/v/websocket_builder.svg)](https://crates.io/crates/websocket_builder)
[![Documentation](https://docs.rs/websocket_builder/badge.svg)](https://docs.rs/websocket_builder)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A low-level middleware-based WebSocket framework for building protocol-aware servers in Rust. This crate provides the foundation for protocol implementations like `nostr_relay_builder`. Designed for building stateful connection pipelines with type-safe message processing.

## Core Features

- Concurrent split-actor architecture for inbound and outbound processing
- Bidirectional middleware pipeline with type-safe message handling
- Fire-and-forget message processing with configurable backpressure
- Per-connection state management with automatic cleanup
- Framework-agnostic design via WebSocket trait abstraction
- Support for both tungstenite and fastwebsockets backends
- Write once, switch backends with feature flags

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
websocket_builder = "0.2.0-alpha.1"
tokio = { version = "1.x", features = ["full"] }
axum = { version = "0.8.x", features = ["ws"] }
async-trait = "0.1.x"
anyhow = "1.0.x"
```

### WebSocket Backends

This library supports two WebSocket implementations that you can switch between using feature flags:

- **Tungstenite** (default) - Mature, widely-used, excellent compatibility
- **FastWebSockets** - High performance, low latency, minimal overhead

```toml
# Use tungstenite (default)
websocket_builder = "0.2.0-alpha.1"

# Use fastwebsockets
websocket_builder = { version = "0.1.0-alpha.1", default-features = false, features = ["fastwebsockets"] }
```

Both backends use the same API, so you can switch between them without changing your code.

## Quick Example: Echo Server with Middleware

```rust
use async_trait::async_trait;
use axum::{
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    InboundContext, MessageConverter, Middleware, SendMessage, StateFactory,
    UnifiedWebSocketExt, WebSocketBuilder, WebSocketUpgrade,
};
use anyhow::Result;

// Define your state
#[derive(Debug, Clone, Default)]
struct AppState {
    name: String,
}

// State factory
#[derive(Clone)]
struct AppStateFactory;

impl StateFactory<Arc<AppState>> for AppStateFactory {
    fn create_state(&self, _token: CancellationToken) -> Arc<AppState> {
        Arc::new(AppState { name: "WebSocket Server".to_string() })
    }
}

// Message converter
#[derive(Clone, Debug)]
struct StringConverter;

impl MessageConverter<String, String> for StringConverter {
    fn inbound_from_string(&self, payload: String) -> Result<Option<String>> {
        Ok(Some(payload))
    }
    fn outbound_to_string(&self, payload: String) -> Result<String> {
        Ok(payload)
    }
}

// Echo middleware
#[derive(Debug)]
struct EchoMiddleware;

#[async_trait]
impl Middleware for EchoMiddleware {
    type State = Arc<AppState>;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(message) = &ctx.message {
            ctx.send_message(format!("Echo: {}", message))?;
        }
        ctx.next().await
    }
}

// WebSocket handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(handler): State<Arc<WebSocketHandler>>,
) -> impl IntoResponse {
    let connection_id = uuid::Uuid::new_v4().to_string();
    let cancellation_token = CancellationToken::new();
    
    handler.start_websocket(ws, connection_id, cancellation_token)
}

type WebSocketHandler = websocket_builder::WebSocketHandler<
    Arc<AppState>,
    String,
    String,
    StringConverter,
    AppStateFactory,
>;

#[tokio::main]
async fn main() {
    let builder = WebSocketBuilder::new(AppStateFactory, StringConverter)
        .with_middleware(EchoMiddleware);
    
    let handler = Arc::new(builder.build());

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(handler);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    
    println!("Server running on ws://127.0.0.1:3000/ws");
    
    axum::serve(listener, app).await.unwrap();
}
```

## Middleware System

Middleware components process messages in a pipeline fashion:

### Message Flow

1. **Inbound**: Client → WebSocket → MessageConverter → Middleware Pipeline → Your Handler
2. **Outbound**: Your Handler → Middleware Pipeline → MessageConverter → WebSocket → Client

### Middleware Lifecycle

```rust
#[async_trait]
impl Middleware for MyMiddleware {
    // Called when connection is established
    async fn on_connect(&self, ctx: &mut ConnectionContext<...>) -> Result<()> {
        // Initialize connection-specific state
        ctx.next().await
    }

    // Process incoming messages
    async fn process_inbound(&self, ctx: &mut InboundContext<...>) -> Result<()> {
        // Transform, filter, or handle messages
        ctx.next().await
    }

    // Process outgoing messages
    async fn process_outbound(&self, ctx: &mut OutboundContext<...>) -> Result<()> {
        // Transform or filter outgoing messages
        ctx.next().await
    }

    // Called when connection closes
    async fn on_disconnect(&self, ctx: &mut DisconnectContext<...>) -> Result<()> {
        // Cleanup
        ctx.next().await
    }
}
```

## Connection Management

The framework automatically handles:
- Connection establishment and teardown
- Graceful disconnection with customizable timeouts
- Backpressure when clients can't keep up
- Error propagation and connection cleanup

### Configuration Options

```rust
let builder = WebSocketBuilder::new(state_factory, message_converter)
    .with_middleware(middleware1)
    .with_middleware(middleware2)
    .with_channel_size(100)  // Channel buffer size
    .with_max_connections(1000)  // Connection limit
    .with_max_connection_time(Duration::from_secs(3600));  // Auto-disconnect after 1 hour
```

## Examples

See the [examples](examples/) directory for:
- `basic_demo.rs` - Simple echo server with middleware
- `pipeline_demo.rs` - Complex middleware pipeline demonstration
- `debug_ws.rs` - WebSocket debugging tool

## Testing

The library supports both unit and integration testing with mock WebSocket connections.

## Status

Early-stage project under active development. Breaking changes should be expected.

## License

MIT