//! WebSocket handler using the WebSocket trait
//!
//! This version uses the WebSocket trait for better testability
//! and framework independence, allowing it to work with any WebSocket implementation.

use crate::{
    split_actors::{process_on_connect, process_on_disconnect, SplitActors, SplitActorsConfig},
    websocket_trait::WebSocketConnection,
    MessageConverter, Middleware,
};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::warn;

// Type alias for middleware collection
type MiddlewareCollection<S, I, O> =
    Arc<Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>>;

/// WebSocket handler with split inbound/outbound processing
///
/// Features:
/// - Concurrent message processing with separate inbound/outbound actors
/// - Optional connection limits via semaphore
/// - Configurable channel sizes for backpressure
pub struct WebSocketHandler<S, I, O, C>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    pub(crate) middlewares: MiddlewareCollection<S, I, O>,
    pub(crate) message_converter: Arc<C>,
    pub(crate) channel_size: usize,
    pub(crate) connection_semaphore: Option<Arc<Semaphore>>,
    pub(crate) max_connection_time: Option<Duration>,
}

impl<S, I, O, C> Clone for WebSocketHandler<S, I, O, C>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            middlewares: self.middlewares.clone(),
            message_converter: self.message_converter.clone(),
            channel_size: self.channel_size,
            connection_semaphore: self.connection_semaphore.clone(),
            max_connection_time: self.max_connection_time,
        }
    }
}

impl<S, I, O, C> WebSocketHandler<S, I, O, C>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    pub(crate) fn new(
        middlewares: Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>,
        message_converter: C,
        channel_size: usize,
        max_connections: Option<usize>,
        max_connection_time: Option<Duration>,
    ) -> Self {
        Self {
            middlewares: Arc::new(middlewares),
            message_converter: Arc::new(message_converter),
            channel_size,
            connection_semaphore: max_connections.map(|cap| Arc::new(Semaphore::new(cap))),
            max_connection_time,
        }
    }

    pub async fn start<W>(
        self: Arc<Self>,
        socket: W,
        connection_id: String,
        cancellation_token: CancellationToken,
        state: S,
    ) -> Result<()>
    where
        W: WebSocketConnection,
        W::Sink: Send + 'static,
        W::Stream: Send + 'static,
        S: Default,
    {
        let connection_token = cancellation_token.child_token();

        // Try to acquire connection permit if max_connections is set
        let _permit = if let Some(semaphore) = &self.connection_semaphore {
            match Arc::clone(semaphore).try_acquire_owned() {
                Ok(permit) => Some(permit),
                Err(_) => {
                    warn!("Maximum connections limit reached, rejecting connection");
                    return Err(anyhow::anyhow!("Maximum connections limit reached"));
                }
            }
        } else {
            None
        };

        // Use the provided state
        let initial_state = state;
        let shared_state = Arc::new(parking_lot::RwLock::new(initial_state));

        // Spawn timeout task if max_connection_time is configured
        if let Some(max_time) = self.max_connection_time {
            let token = connection_token.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::time::sleep(max_time) => {
                        if !token.is_cancelled() {
                            warn!(
                                "Max connection time ({:?}) exceeded, disconnecting",
                                max_time
                            );
                            token.cancel();
                        }
                    }
                    _ = token.cancelled() => {} // Connection already cancelled
                }
            });
        }

        // Split the WebSocket
        let (ws_sink, ws_stream) = socket.split();

        // Create config for split actors
        let split_actors_config = SplitActorsConfig {
            state: shared_state.clone(),
            middlewares: self.middlewares.clone(),
            connection_id: connection_id.clone(),
            channel_size: self.channel_size,
            message_converter: self.message_converter.clone() as Arc<dyn MessageConverter<I, O>>,
            cancellation_token: connection_token.clone(),
        };

        // Spawn split actors with direct WebSocket I/O
        let outbound_sender = SplitActors::spawn(split_actors_config, ws_stream, ws_sink);

        // Process on_connect event
        process_on_connect(
            shared_state.clone(),
            &self.middlewares,
            &connection_id,
            &outbound_sender,
        )
        .await?;

        // Wait for cancellation (actors handle their own lifecycle)
        connection_token.cancelled().await;

        // Process on_disconnect event directly
        process_on_disconnect(
            shared_state.clone(),
            &self.middlewares,
            &connection_id,
            outbound_sender,
        )
        .await
        .ok(); // Ignore errors during disconnect

        // Actors will shut down when their channels are dropped
        Ok(())
    }
}

/// Builder for the WebSocket handler
pub struct WebSocketBuilder<S, I, O, C>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    middlewares: Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>,
    message_converter: C,
    channel_size: usize,
    max_connections: Option<usize>,
    max_connection_time: Option<Duration>,
}

impl<S, I, O, C> WebSocketBuilder<S, I, O, C>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    pub fn new(message_converter: C) -> Self {
        Self {
            middlewares: Vec::new(),
            message_converter,
            channel_size: 100,
            max_connections: None,
            max_connection_time: None,
        }
    }

    pub fn with_middleware<M>(mut self, middleware: M) -> Self
    where
        M: Middleware<State = S, IncomingMessage = I, OutgoingMessage = O> + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    pub fn with_arc_middleware(
        mut self,
        middleware: Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>,
    ) -> Self {
        self.middlewares.push(middleware);
        self
    }

    pub fn with_channel_size(mut self, size: usize) -> Self {
        self.channel_size = size;
        self
    }

    /// Sets the maximum number of concurrent connections.
    ///
    /// When this limit is reached, new connection attempts will be
    /// rejected with a `MaxConnectionsExceeded` error.
    ///
    /// # Arguments
    /// * `max` - The maximum number of concurrent connections
    ///
    /// # Returns
    /// The builder instance for method chaining
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = Some(max);
        self
    }

    /// Sets the maximum duration for a connection.
    ///
    /// After this duration, the connection will be gracefully closed.
    /// This can be used to implement connection rotation or to prevent
    /// resource leaks from long-lived connections.
    ///
    /// # Arguments
    /// * `duration` - The maximum duration for a connection
    ///
    /// # Returns
    /// The builder instance for method chaining
    pub fn with_max_connection_time(mut self, duration: Duration) -> Self {
        self.max_connection_time = Some(duration);
        self
    }

    pub fn build(self) -> WebSocketHandler<S, I, O, C> {
        WebSocketHandler::new(
            self.middlewares,
            self.message_converter,
            self.channel_size,
            self.max_connections,
            self.max_connection_time,
        )
    }
}
