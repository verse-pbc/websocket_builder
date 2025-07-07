//! Example showing flexible WebSocket handling with Option<WebSocketUpgrade>
//!
//! This demonstrates how the same endpoint can handle both regular HTTP and WebSocket requests.
//! Works identically with both tungstenite and fastwebsockets backends.

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    extract::ConnectInfo,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    InboundContext, Middleware, SendMessage, StringConverter, UnifiedWebSocketExt,
    WebSocketBuilder, WebSocketUpgrade,
};

// Per-connection state (empty for this example)
#[derive(Debug, Clone, Default)]
struct ConnectionState;

// Use the built-in StringConverter

// Echo middleware
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
            ctx.send_message(format!("Echo: {message}"))?;
        }
        ctx.next().await
    }
}

type Handler =
    websocket_builder::WebSocketHandler<ConnectionState, String, String, StringConverter>;

// Flexible handler that accepts both HTTP and WebSocket requests
// The key insight: we need to structure this as a regular function, not a closure
async fn flexible_handler(
    axum::extract::State(handler): axum::extract::State<Arc<Handler>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    ws: Option<WebSocketUpgrade>,
) -> Response {
    match ws {
        Some(ws) => {
            // Handle WebSocket upgrade
            let connection_id = addr.to_string(); // Use actual IP:port
            let cancellation_token = CancellationToken::new();
            println!("WebSocket connection from: {connection_id}");
            let state = ConnectionState;
            handler
                .handle_upgrade(ws, connection_id, cancellation_token, state)
                .await
        }
        None => {
            // Handle regular HTTP request
            Html(
                r#"
                <!DOCTYPE html>
                <html>
                <head>
                    <title>WebSocket Test</title>
                </head>
                <body>
                    <h1>WebSocket Test Page</h1>
                    <p>This endpoint supports both HTTP and WebSocket connections!</p>
                    <p>Open the browser console and run:</p>
                    <pre>
const ws = new WebSocket('ws://' + window.location.host + '/');
ws.onmessage = (e) => console.log('Received:', e.data);
ws.onopen = () => {
    console.log('Connected!');
    ws.send('Hello from browser!');
};
                    </pre>
                    <p>Or test with: <code>websocat ws://127.0.0.1:3003/</code></p>
                </body>
                </html>
            "#,
            )
            .into_response()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Build the handler
    let handler = Arc::new(
        WebSocketBuilder::new(StringConverter::new())
            .with_middleware(EchoMiddleware)
            .build(),
    );

    // Create the app - same endpoint handles both HTTP and WebSocket requests
    let app = Router::new()
        .route("/", get(flexible_handler))
        .with_state(handler);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003").await?;

    println!("Flexible server running on http://127.0.0.1:3003");
    println!("- Visit http://127.0.0.1:3003 in a browser for the test page");
    println!("- Connect via WebSocket: websocat ws://127.0.0.1:3003/");
    println!();
    println!("This demonstrates Option<WebSocketUpgrade> for flexible endpoints:");

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
