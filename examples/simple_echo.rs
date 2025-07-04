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
    InboundContext, Middleware, SendMessage, StateFactory, StringConverter, WebSocketBuilder,
};

// Minimal state
#[derive(Debug, Clone, Default)]
struct State;

#[derive(Clone)]
struct StateFactory_;

impl StateFactory<Arc<State>> for StateFactory_ {
    fn create_state(&self, _token: CancellationToken) -> Arc<State> {
        Arc::new(State)
    }
}

// Use the built-in StringConverter

// Simple echo middleware
#[derive(Debug)]
struct EchoMiddleware;

#[async_trait]
impl Middleware for EchoMiddleware {
    type State = Arc<State>;
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
        WebSocketBuilder::new(StateFactory_, StringConverter::new())
            .with_middleware(EchoMiddleware)
            .build(),
    );

    // Create the app - simple handler function
    async fn ws_handler(
        ws: websocket_builder::WebSocketUpgrade,
        ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
        handler: Arc<
            websocket_builder::WebSocketHandler<
                Arc<State>,
                String,
                String,
                StringConverter,
                StateFactory_,
            >,
        >,
    ) -> axum::response::Response {
        use websocket_builder::UnifiedWebSocketExt;
        let connection_id = addr.to_string(); // Use actual IP:port
        let cancellation_token = CancellationToken::new();
        println!("Echo connection from: {connection_id}");
        handler
            .handle_upgrade(ws, connection_id, cancellation_token)
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
