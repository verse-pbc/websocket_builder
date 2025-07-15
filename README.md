# WebSocket Builder

[![Crates.io](https://img.shields.io/crates/v/websocket_builder.svg)](https://crates.io/crates/websocket_builder)
[![Documentation](https://docs.rs/websocket_builder/badge.svg)](https://docs.rs/websocket_builder)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A middleware-based WebSocket framework for Rust. Used as the foundation for `nostr_relay_builder`.

## Installation

```toml
[dependencies]
websocket_builder = "0.2.0-alpha.1"
```

## Features

- Middleware pipeline for message processing
- Per-connection state management
- Configurable backpressure and connection limits
- Support for tungstenite (default) and fastwebsockets backends

## Quick Start

```rust
use websocket_builder::{
    WebSocketBuilder, MessageConverterTrait, Middleware,
    InboundContext, SendMessage
};
use async_trait::async_trait;
use anyhow::Result;

// Define per-connection state
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

// Create a middleware
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
        if let Some(msg) = &ctx.message {
            ctx.send_message(format!("Echo: {}", msg))?;
        }
        ctx.next().await
    }
}

// Build and use
let handler = WebSocketBuilder::new(StringConverter)
    .with_middleware(EchoMiddleware)
    .build();
```

## Examples

- `examples/simple_echo.rs` - Minimal echo server
- `examples/pipeline_demo.rs` - Multiple middleware in sequence  
- `examples/basic_demo.rs` - Connection state tracking
- `examples/flexible_handler.rs` - HTTP and WebSocket on same endpoint
- `examples/debug_ws.rs` - WebSocket debugging client

## Configuration

```rust
use std::time::Duration;

WebSocketBuilder::new(StringConverter)
    .with_middleware(middleware)
    .with_channel_size(100)
    .with_max_connections(1000)
    .with_max_connection_time(Duration::from_secs(3600))
    .build()
```

## License

MIT