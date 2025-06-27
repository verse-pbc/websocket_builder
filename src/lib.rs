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
//! use websocket_builder::{WebSocketBuilder, Middleware, InboundContext, StateFactory, MessageConverter, SendMessage};
//! use async_trait::async_trait;
//! use anyhow::Result;
//! use std::sync::Arc;
//! use tokio_util::sync::CancellationToken;
//!
//! // Define a simple state type
//! #[derive(Debug, Clone, Default)]
//! struct MyState;
//!
//! // State factory to create state instances
//! #[derive(Clone)]
//! struct MyStateFactory;
//!
//! impl StateFactory<Arc<MyState>> for MyStateFactory {
//!     fn create_state(&self, _token: CancellationToken) -> Arc<MyState> {
//!         Arc::new(MyState::default())
//!     }
//! }
//!
//! // Message converter for string messages
//! #[derive(Clone, Debug)]
//! struct StringConverter;
//!
//! impl MessageConverter<String, String> for StringConverter {
//!     fn inbound_from_string(&self, payload: String) -> Result<Option<String>> {
//!         Ok(Some(payload))
//!     }
//!     fn outbound_to_string(&self, payload: String) -> Result<String> {
//!         Ok(payload)
//!     }
//! }
//!
//! // Echo middleware that sends messages back
//! #[derive(Debug)]
//! struct EchoMiddleware;
//!
//! #[async_trait]
//! impl Middleware for EchoMiddleware {
//!     type State = Arc<MyState>;
//!     type IncomingMessage = String;
//!     type OutgoingMessage = String;
//!
//!     async fn process_inbound(
//!         &self,
//!         ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
//!     ) -> Result<()> {
//!         // Echo the message back
//!         if let Some(message) = &ctx.message {
//!             ctx.send_message(message.clone())?;
//!         }
//!         ctx.next().await
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let builder = WebSocketBuilder::new(MyStateFactory, StringConverter)
//!         .with_middleware(EchoMiddleware);
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
