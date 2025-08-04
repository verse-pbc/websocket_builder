//! # WebSocket Builder
//!
//! A simple, trait-based WebSocket framework for Rust with Axum integration.

// Enable strict clippy lints for better performance and code quality
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
// Performance-specific lints
#![warn(clippy::unnecessary_to_owned)]
#![warn(clippy::redundant_clone)]
#![warn(clippy::inefficient_to_string)]
#![warn(clippy::manual_str_repeat)]
// Allow some pedantic lints that might be too strict
#![allow(clippy::module_name_repetitions)] // Common in Rust APIs
#![allow(clippy::must_use_candidate)] // Not all functions need #[must_use]
#![allow(clippy::missing_errors_doc)] // Can add later if needed
#![allow(clippy::missing_panics_doc)] // Can add later if needed
#![allow(clippy::multiple_crate_versions)] // Dependencies manage their own versions
#![allow(clippy::option_if_let_else)] // Sometimes match is clearer
#![allow(clippy::type_complexity)] // Will be addressed in future refactoring
#![allow(clippy::single_match_else)] // Sometimes match is clearer
#![allow(clippy::cognitive_complexity)] // Will be addressed in future refactoring
//!
//! ## Overview
//!
//! `websocket_builder` provides a minimal abstraction over WebSocket connections:
//! - Simple trait-based handler with just 3 methods
//! - Per-connection handler instances for natural state management
//! - Automatic handling of WebSocket protocol details
//! - Easy integration with Axum
//!
//! ## Quick Example
//!
//! ```rust,no_run
//! use websocket_builder::{WebSocketHandler, HandlerFactory, DisconnectReason, Utf8Bytes, websocket_route};
//! use axum::{extract::ws::{Message, WebSocket}, Router, routing::get};
//! use futures_util::{stream::SplitSink, SinkExt};
//! use std::net::SocketAddr;
//! use anyhow::Result;
//!
//! struct ChatHandler {
//!     addr: SocketAddr,
//!     sink: Option<SplitSink<WebSocket, Message>>,
//! }
//!
//! impl WebSocketHandler for ChatHandler {
//!     async fn on_connect(
//!         &mut self,
//!         addr: SocketAddr,
//!         sink: SplitSink<WebSocket, Message>,
//!     ) -> Result<()> {
//!         self.addr = addr;
//!         self.sink = Some(sink);
//!         
//!         // Send welcome message
//!         if let Some(sink) = &mut self.sink {
//!             sink.send(Message::Text("Welcome to the chat!".into())).await?;
//!         }
//!         Ok(())
//!     }
//!     
//!     async fn on_message(&mut self, text: Utf8Bytes) -> Result<()> {
//!         // Echo the message back
//!         if let Some(sink) = &mut self.sink {
//!             sink.send(Message::Text(format!("Echo: {}", text).into())).await?;
//!         }
//!         Ok(())
//!     }
//!     
//!     async fn on_disconnect(&mut self, reason: DisconnectReason) {
//!         println!("Client {} disconnected: {:?}", self.addr, reason);
//!     }
//! }
//!
//! struct ChatHandlerFactory;
//!
//! impl HandlerFactory for ChatHandlerFactory {
//!     type Handler = ChatHandler;
//!     
//!     fn create(&self, _headers: &axum::http::HeaderMap) -> Self::Handler {
//!         ChatHandler {
//!             addr: "0.0.0.0:0".parse().unwrap(),
//!             sink: None,
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let app = websocket_route("/ws", ChatHandlerFactory);
//!     
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
//!     axum::serve(listener, app).await.unwrap();
//! }
//! ```

mod axum;
mod connection;
mod handler;

// Re-export the public API
pub use axum::{
    handle_upgrade, handle_upgrade_with_config, websocket_route, websocket_route_with_config,
    WebSocketUpgrade,
};
pub use connection::{handle_socket, ConnectionConfig};
pub use handler::{DisconnectReason, HandlerFactory, Utf8Bytes, WebSocketHandler};
