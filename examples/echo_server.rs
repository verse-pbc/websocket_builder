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
use std::net::SocketAddr;
use websocket_builder::{
    websocket_route, DisconnectReason, HandlerFactory, Utf8Bytes, WebSocketHandler,
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
        println!("Client {} disconnected: {reason:?}", self.addr);
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

    // Build our application
    let ws_app = websocket_route("/ws", EchoHandlerFactory);
    let app = Router::new()
        .route(
            "/",
            axum::routing::get(|| async { "WebSocket Echo Server - Connect to /ws" }),
        )
        .merge(ws_app);

    // Run it
    let addr = "127.0.0.1:3000";
    println!("Listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
