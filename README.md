# WebSocket Builder

[![Crates.io](https://img.shields.io/crates/v/websocket_builder.svg)](https://crates.io/crates/websocket_builder)
[![Documentation](https://docs.rs/websocket_builder/badge.svg)](https://docs.rs/websocket_builder)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A simple, trait-based WebSocket framework for Rust with Axum integration.

## Features

- 🎯 **Simple trait-based API** - Just implement 3 methods: `on_connect`, `on_message`, `on_disconnect`
- 🔌 **Per-connection handlers** - Each connection gets its own handler instance for natural state management
- 🚀 **Axum integration** - Easy setup with Axum web framework
- 📦 **Minimal dependencies** - Only what's needed for WebSocket handling
- 🛡️ **Connection limits** - Built-in support for limiting concurrent connections
- 🔄 **Automatic protocol handling** - Ping/pong and close frames handled automatically

## Installation

```toml
[dependencies]
websocket_builder = "1.1.0-alpha.1"
```

## Quick Start

```rust
use websocket_builder::{WebSocketHandler, HandlerFactory, DisconnectReason, Utf8Bytes, websocket_route};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{stream::SplitSink, SinkExt};
use std::net::SocketAddr;
use anyhow::Result;

// Define your connection handler
struct MyHandler {
    addr: SocketAddr,
    sink: Option<SplitSink<WebSocket, Message>>,
}

impl WebSocketHandler for MyHandler {
    async fn on_connect(
        &mut self,
        addr: SocketAddr,
        sink: SplitSink<WebSocket, Message>,
    ) -> Result<()> {
        self.addr = addr;
        self.sink = Some(sink);
        
        // Send welcome message
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text("Welcome!".into())).await?;
        }
        Ok(())
    }
    
    async fn on_message(&mut self, text: Utf8Bytes) -> Result<()> {
        // Echo the message back
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text(format!("Echo: {}", text).into())).await?;
        }
        Ok(())
    }
    
    async fn on_disconnect(&mut self, reason: DisconnectReason) {
        println!("Client {} disconnected: {:?}", self.addr, reason);
    }
}

// Factory to create handler instances
struct MyHandlerFactory;

impl HandlerFactory for MyHandlerFactory {
    type Handler = MyHandler;
    
    fn create(&self, _headers: &axum::http::HeaderMap) -> Self::Handler {
        MyHandler {
            addr: "0.0.0.0:0".parse().unwrap(),
            sink: None,
        }
    }
}

// Create your Axum app
#[tokio::main]
async fn main() {
    let app = websocket_route("/ws", MyHandlerFactory);
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

## Examples

Check out the examples directory for more complete examples:

- `echo_server.rs` - Simple echo server
- `chat_server.rs` - Multi-user chat room with usernames
- `flexible_handler.rs` - Same route handling both HTTP and WebSocket

Run an example:
```bash
cargo run --example echo_server
```

## How It Works

1. **Create a handler** - Implement the `WebSocketHandler` trait with your connection logic
2. **Create a factory** - Implement `HandlerFactory` to create handler instances for each connection
3. **Set up routes** - Use `websocket_route()` to create an Axum router with your WebSocket endpoint
4. **Handle connections** - Each connection gets its own handler instance, perfect for maintaining state

## Configuration

You can configure connection limits:

```rust
use websocket_builder::{websocket_route_with_config, ConnectionConfig};

let config = ConnectionConfig {
    max_connections: Some(1000),
};

let app = websocket_route_with_config("/ws", MyHandlerFactory, config);
```

## Advanced Usage

### Flexible Handler Pattern

You can create routes that handle both regular HTTP and WebSocket requests using `Option<WebSocketUpgrade>`:

```rust
use websocket_builder::{WebSocketUpgrade, handle_upgrade};
use axum::{
    extract::ConnectInfo,
    response::{Html, IntoResponse, Response}
};

async fn flexible_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    ws: Option<WebSocketUpgrade>,
) -> Response {
    match ws {
        Some(ws) => {
            // Handle WebSocket upgrade
            let handler = MyHandlerFactory.create(&headers);
            handle_upgrade(ws, addr, handler).await
        }
        None => {
            // Handle regular HTTP request
            Html("<h1>Hello from HTTP!</h1>").into_response()
        }
    }
}
```

See the `flexible_handler.rs` example for a complete implementation that serves both HTML and WebSocket on the same route.

### Using handle_socket Directly

For even more control, you can use `handle_socket` directly with Axum's WebSocket:

```rust
use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    let handler = factory.create();
    ws.on_upgrade(move |socket| async move {
        websocket_builder::handle_socket(socket, addr, handler, config).await
    })
}
```

## Design Philosophy

This library prioritizes simplicity over flexibility. If you need:
- Complex middleware chains
- Multiple message types
- Binary message handling
- Custom protocol extensions

You might want to use the underlying Axum WebSocket functionality directly for more control.

## License

MIT