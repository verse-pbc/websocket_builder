//! # WebSocket Builder
//!
//! A flexible WebSocket framework for building scalable, middleware-based WebSocket servers.
//!
//! ## Overview
//!
//! `websocket_builder` provides a high-level abstraction over WebSocket connections with:
//! - Middleware-based message processing pipeline
//! - Built-in connection management and lifecycle handling
//! - Backpressure support for handling slow clients
//! - Type-safe message conversion and routing
//! - Integration with Axum web framework
//!
//! ## Quick Example
//!
//! ```rust,no_run
//! use websocket_builder::{WebSocketBuilder, Middleware, InboundContext, OutboundContext};
//! use async_trait::async_trait;
//!
//! struct EchoMiddleware;
//!
//! #[async_trait]
//! impl Middleware for EchoMiddleware {
//!     async fn handle_message(&self, ctx: &InboundContext) -> Result<(), websocket_builder::WebsocketError> {
//!         // Echo the message back
//!         ctx.send_message(ctx.message().clone()).await
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut builder = WebSocketBuilder::new();
//!     builder.add_middleware(EchoMiddleware);
//!     
//!     // Use with Axum or other web frameworks
//!     let handler = builder.build();
//! }
//! ```
//!
//! ## Core Concepts
//!
//! ### Middleware
//!
//! Middleware components process messages in a pipeline fashion. Each middleware can:
//! - Handle inbound messages from clients
//! - Transform or filter outbound messages to clients
//! - Manage connection lifecycle events (connect/disconnect)
//! - Maintain per-connection state
//!
//! ### Message Flow
//!
//! 1. **Inbound**: Client → WebSocket → MessageConverter → Middleware Pipeline → Your Handler
//! 2. **Outbound**: Your Handler → Middleware Pipeline → MessageConverter → WebSocket → Client
//!
//! ### Connection Management
//!
//! The framework automatically handles:
//! - Connection establishment and teardown
//! - Graceful disconnection with customizable timeouts
//! - Backpressure when clients can't keep up
//! - Error propagation and connection cleanup
//!
//! ## Features
//!
//! - **Flexible Middleware System**: Compose reusable message processing components
//! - **Type Safety**: Generic over message types with built-in conversion traits
//! - **Performance**: Efficient message routing with minimal allocations
//! - **Error Handling**: Comprehensive error types with context preservation
//! - **Testing Support**: Utilities for testing middleware and handlers
//!
//! ## Advanced Usage
//!
//! See the [examples](https://github.com/verse-pbc/websocket_builder/tree/main/examples) directory
//! for more complex scenarios including:
//! - Authentication middleware
//! - Rate limiting
//! - Message routing
//! - State management
//! - Custom protocols

pub mod middleware;
pub mod middleware_context;
mod split_actors; // Internal implementation detail
pub mod websocket_handler;
pub mod websocket_trait;

pub use middleware::Middleware;
pub use middleware_context::{
    ConnectionContext, DisconnectContext, InboundContext, MessageConverter, MessageSender,
    MiddlewareVec, OutboundContext, SendMessage, SharedMiddlewareVec, StateFactory, WebsocketError,
};
pub use websocket_handler::{AxumWebSocketExt, WebSocketBuilder, WebSocketHandler};
pub use websocket_trait::{
    AxumWebSocket, WebSocketConnection, WsError, WsMessage, WsSink, WsStream, WsStreamFuture,
};
