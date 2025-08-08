//! Simple echo server example
//!
//! This example shows a basic WebSocket echo server that sends back
//! any message it receives.

use anyhow::Result;
use axum::{
    extract::ws::{Message, WebSocket},
    Router,
};
use futures_util::{stream::SplitSink, SinkExt};
use std::{net::SocketAddr, time::Duration};
use websocket_builder::{
    websocket_route_with_config, ConnectionConfig, DisconnectReason, HandlerFactory, Utf8Bytes,
    WebSocketHandler,
};

/// Simple echo handler
struct EchoHandler {
    addr: SocketAddr,
    sink: Option<SplitSink<WebSocket, Message>>,
}

impl WebSocketHandler for EchoHandler {
    async fn on_connect(
        &mut self,
        addr: SocketAddr,
        sink: SplitSink<WebSocket, Message>,
    ) -> Result<()> {
        self.addr = addr;
        self.sink = Some(sink);
        println!("Client connected from {addr}");

        // Send welcome message
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text("Welcome to the echo server!".into()))
                .await?;
        }
        Ok(())
    }

    async fn on_message(&mut self, text: Utf8Bytes) -> Result<()> {
        println!("Received from {}: {text}", self.addr);

        // Echo the message back
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text(format!("Echo: {text}").into()))
                .await?;
        }
        Ok(())
    }

    async fn on_disconnect(&mut self, reason: DisconnectReason) {
        match &reason {
            DisconnectReason::Timeout(msg) => {
                println!("Client {} timed out: {}", self.addr, msg);
            }
            _ => {
                println!("Client {} disconnected: {reason:?}", self.addr);
            }
        }
    }
}

/// Factory for creating echo handlers
struct EchoHandlerFactory;

impl HandlerFactory for EchoHandlerFactory {
    type Handler = EchoHandler;

    fn create(&self, _headers: &axum::http::HeaderMap) -> Self::Handler {
        EchoHandler {
            addr: "0.0.0.0:0".parse().unwrap(),
            sink: None,
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Configure connection timeouts
    // Uncomment and adjust these values to enable timeouts:
    let config = ConnectionConfig {
        max_connections: Some(100),    // Maximum 100 concurrent connections
        max_connection_duration: None, // Example: Some(Duration::from_secs(300)) for 5 minutes
        idle_timeout: None, // Example: Some(Duration::from_secs(60)) for 1 minute idle timeout
    };

    // Build our application with timeout configuration
    // Use websocket_route_with_config to apply the configuration
    let ws_app = websocket_route_with_config("/ws", EchoHandlerFactory, config.clone());

    // Also demonstrate a route with different timeout settings
    let strict_config = ConnectionConfig {
        max_connections: Some(10),
        max_connection_duration: Some(Duration::from_secs(120)), // 2 minute max
        idle_timeout: Some(Duration::from_secs(30)),             // 30 second idle timeout
    };
    let strict_ws_app =
        websocket_route_with_config("/ws-strict", EchoHandlerFactory, strict_config);

    let app = Router::new()
        .route(
            "/",
            axum::routing::get(|| async {
                "WebSocket Echo Server\n\
                 - Connect to /ws for normal connections\n\
                 - Connect to /ws-strict for connections with timeout limits"
            }),
        )
        .merge(ws_app)
        .merge(strict_ws_app);

    // Run it
    let addr = "127.0.0.1:3000";
    println!("Listening on http://{addr}");
    println!("WebSocket endpoints:");
    println!("  /ws        - Normal connections (no timeouts)");
    println!("  /ws-strict - Strict timeouts (2 min max, 30s idle)");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
