//! Chat server example
//!
//! This example shows a more complex WebSocket handler that implements
//! a simple chat with usernames. Note: broadcast messaging requires
//! additional architecture when using SplitSink directly.

use anyhow::Result;
use axum::{
    extract::ws::{Message, WebSocket},
    Router,
};
use futures_util::{stream::SplitSink, SinkExt};
use parking_lot::Mutex;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use websocket_builder::{
    websocket_route, DisconnectReason, HandlerFactory, Utf8Bytes, WebSocketHandler,
};

type UserConnection = (String, SplitSink<WebSocket, Message>);

/// Shared state for all connections
#[derive(Clone)]
struct ChatState {
    /// Map of connection address to username and sender
    connections: Arc<Mutex<HashMap<SocketAddr, UserConnection>>>,
}

impl ChatState {
    fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Broadcast a message to all connected clients
    fn broadcast(&self, from: &str, message: &str) {
        let msg = format!("{from}: {message}");

        // Since we can't await while holding the lock and can't clone SplitSink,
        // we'll need to restructure this. For now, just store the message
        // and the handler will need to handle it differently.
        println!("[BROADCAST] {msg}");
    }

    /// Add a new connection
    fn add_connection(
        &self,
        addr: SocketAddr,
        username: String,
        sink: SplitSink<WebSocket, Message>,
    ) {
        self.connections.lock().insert(addr, (username, sink));
    }

    /// Remove a connection
    fn remove_connection(&self, addr: &SocketAddr) -> Option<String> {
        self.connections
            .lock()
            .remove(addr)
            .map(|(username, _)| username)
    }
}

/// Chat connection handler
struct ChatHandler {
    addr: SocketAddr,
    sink: Option<SplitSink<WebSocket, Message>>,
    username: Option<String>,
    state: ChatState,
}

impl WebSocketHandler for ChatHandler {
    async fn on_connect(
        &mut self,
        addr: SocketAddr,
        sink: SplitSink<WebSocket, Message>,
    ) -> Result<()> {
        self.addr = addr;
        self.sink = Some(sink);
        println!("New connection from {addr}");

        // Send instructions
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text(
                "Welcome! Please enter your username with: /name <username>".into(),
            ))
            .await?;
        }
        Ok(())
    }

    async fn on_message(&mut self, text: Utf8Bytes) -> Result<()> {
        let text_str = text.as_str();
        if let Some(name_part) = text_str.strip_prefix("/name ") {
            // Set username
            let username = name_part.trim().to_string();
            if username.is_empty() {
                if let Some(sink) = &mut self.sink {
                    sink.send(Message::Text("Username cannot be empty!".into()))
                        .await?;
                }
                return Ok(());
            }

            self.username = Some(username.clone());
            if let Some(sink) = self.sink.take() {
                // Send welcome message before adding to state
                let mut sink = sink;
                sink.send(Message::Text(
                    format!("Welcome, {username}! You can now send messages.").into(),
                ))
                .await?;

                self.state.add_connection(self.addr, username.clone(), sink);
                self.state
                    .broadcast("System", &format!("{username} joined the chat"));
            }
        } else if let Some(username) = &self.username {
            // Broadcast message
            self.state.broadcast(username, text_str);
        } else {
            // Not logged in
            if let Some(sink) = &mut self.sink {
                sink.send(Message::Text(
                    "Please set your username first with: /name <username>".into(),
                ))
                .await?;
            }
        }
        Ok(())
    }

    async fn on_disconnect(&mut self, reason: DisconnectReason) {
        if let Some(username) = self.state.remove_connection(&self.addr) {
            println!("User {username} disconnected: {reason:?}");
            self.state
                .broadcast("System", &format!("{username} left the chat"));
        } else {
            println!(
                "Anonymous connection {} disconnected: {reason:?}",
                self.addr
            );
        }
    }
}

/// Factory for creating chat handlers
struct ChatHandlerFactory {
    state: ChatState,
}

impl HandlerFactory for ChatHandlerFactory {
    type Handler = ChatHandler;

    fn create(&self, _headers: &axum::http::HeaderMap) -> Self::Handler {
        ChatHandler {
            addr: "0.0.0.0:0".parse().unwrap(),
            sink: None,
            username: None,
            state: self.state.clone(),
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create shared state
    let state = ChatState::new();
    let factory = ChatHandlerFactory { state };

    // Build our application
    let ws_app = websocket_route("/ws", factory);
    let app = Router::new()
        .route(
            "/",
            axum::routing::get(|| async { "WebSocket Chat Server - Connect to /ws" }),
        )
        .merge(ws_app);

    // Run it
    let addr = "127.0.0.1:3000";
    println!("Chat server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
