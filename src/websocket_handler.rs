//! WebSocket handler using the WebSocket trait
//!
//! This version uses the WebSocket trait for better testability
//! and framework independence, allowing it to work with any WebSocket implementation.

use crate::{
    split_actors::{process_on_connect, process_on_disconnect, SplitActors},
    websocket_trait::{AxumWebSocket, WebSocketConnection, WsMessage, WsSink, WsStream},
    MessageConverter, Middleware, StateFactory,
};
use anyhow::Result;
use flume;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

// Type alias for middleware collection
type MiddlewareCollection<S, I, O> =
    Arc<Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>>;

/// WebSocket handler with split inbound/outbound processing
///
/// Features:
/// - Concurrent message processing with separate inbound/outbound actors
/// - Optional connection limits via semaphore
/// - Configurable channel sizes for backpressure
pub struct WebSocketHandler<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    middlewares: MiddlewareCollection<S, I, O>,
    message_converter: Arc<C>,
    state_factory: F,
    channel_size: usize,
    connection_semaphore: Option<Arc<Semaphore>>,
    max_connection_time: Option<Duration>,
}

impl<S, I, O, C, F> Clone for WebSocketHandler<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            middlewares: self.middlewares.clone(),
            message_converter: self.message_converter.clone(),
            state_factory: self.state_factory.clone(),
            channel_size: self.channel_size,
            connection_semaphore: self.connection_semaphore.clone(),
            max_connection_time: self.max_connection_time,
        }
    }
}

impl<S, I, O, C, F> WebSocketHandler<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    pub fn new(
        middlewares: Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>,
        message_converter: C,
        state_factory: F,
        channel_size: usize,
        max_connections: Option<usize>,
        max_connection_time: Option<Duration>,
    ) -> Self {
        Self {
            middlewares: Arc::new(middlewares),
            message_converter: Arc::new(message_converter),
            state_factory,
            channel_size,
            connection_semaphore: max_connections.map(|cap| Arc::new(Semaphore::new(cap))),
            max_connection_time,
        }
    }

    pub async fn start<W>(
        &self,
        socket: W,
        connection_id: String,
        cancellation_token: CancellationToken,
    ) -> Result<()>
    where
        W: WebSocketConnection,
        W::Sink: Send + 'static,
        W::Stream: Send + 'static,
    {
        let connection_token = cancellation_token.child_token();

        // Try to acquire connection permit if max_connections is set
        let _permit = if let Some(semaphore) = &self.connection_semaphore {
            match semaphore.clone().try_acquire_owned() {
                Ok(permit) => Some(permit),
                Err(_) => {
                    warn!("Maximum connections limit reached, rejecting connection");
                    return Err(anyhow::anyhow!("Maximum connections limit reached"));
                }
            }
        } else {
            None
        };

        // Create initial state wrapped in Arc<RwLock<>>
        let initial_state = self.state_factory.create_state(connection_token.clone());
        let shared_state = Arc::new(tokio::sync::RwLock::new(initial_state));

        // Spawn timeout task if max_connection_time is configured
        if let Some(max_time) = self.max_connection_time {
            let token = connection_token.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::time::sleep(max_time) => {
                        if !token.is_cancelled() {
                            warn!(
                                "Max connection time ({:?}) exceeded, initiating graceful shutdown",
                                max_time
                            );
                            token.cancel();
                        }
                    }
                    _ = token.cancelled() => {} // Connection already cancelled
                }
            });
        }

        // Create channel for writer task
        let (websocket_tx, websocket_rx) = flume::bounded::<String>(self.channel_size);

        // Spawn split actors
        let actors = SplitActors::spawn(
            shared_state.clone(),
            self.middlewares.clone(),
            connection_id.clone(),
            self.channel_size,
            websocket_tx,
            self.message_converter.clone() as Arc<dyn MessageConverter<I, O>>,
        );

        // Process on_connect event
        process_on_connect(
            shared_state.clone(),
            &self.middlewares,
            &connection_id,
            &actors.outbound_sender,
        )
        .await?;

        // Split the WebSocket
        let (ws_sink, ws_stream) = socket.split();

        // Spawn reader task
        let reader_handle = self.spawn_reader(
            connection_id.clone(),
            ws_stream,
            actors.inbound_sender.clone(),
            connection_token.clone(),
        );

        // Spawn writer task
        let writer_handle = self.spawn_writer(
            connection_id.clone(),
            ws_sink,
            websocket_rx,
            connection_token.clone(),
        );

        // Wait for tasks to complete
        tokio::select! {
            _ = reader_handle => {
                debug!("Reader task completed");
            }
            _ = writer_handle => {
                debug!("Writer task completed");
            }
            _ = connection_token.cancelled() => {
                debug!("Connection cancelled");
            }
        }

        // Process on_disconnect event directly
        process_on_disconnect(
            shared_state.clone(),
            &self.middlewares,
            &connection_id,
            &actors.outbound_sender,
        )
        .await
        .ok(); // Ignore errors during disconnect

        // Actors will shut down when their channels are dropped

        Ok(())
    }

    fn spawn_reader<Str>(
        &self,
        connection_id: String,
        mut ws_stream: Str,
        inbound_sender: flume::Sender<(String, I)>,
        cancellation_token: CancellationToken,
    ) -> tokio::task::JoinHandle<()>
    where
        Str: WsStream + Send + 'static,
    {
        let message_converter = self.message_converter.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        break;
                    }

                    message = ws_stream.next() => {
                        match message {
                            Some(Ok(WsMessage::Text(text))) => {
                                match message_converter.inbound_from_string(text) {
                                    Ok(Some(msg)) => {
                                        // Send directly to InboundActor
                                        if inbound_sender.send((connection_id.clone(), msg)).is_err() {
                                            error!("InboundActor channel closed");
                                            break;
                                        }
                                    }
                                    Ok(None) => {
                                        // Message filtered out
                                    }
                                    Err(e) => {
                                        error!("Failed to convert inbound message: {}", e);
                                    }
                                }
                            }
                            Some(Ok(WsMessage::Binary(data))) => {
                                error!("Protocol violation: received binary message with {} bytes - terminating connection as binary messages are not supported", data.len());
                                break;
                            }
                            Some(Ok(WsMessage::Ping(data))) => {
                                debug!("Received ping with {} bytes", data.len());
                            }
                            Some(Ok(WsMessage::Pong(data))) => {
                                debug!("Received pong with {} bytes", data.len());
                            }
                            Some(Ok(WsMessage::Close(_))) => {
                                debug!("WebSocket closed by client");
                                break;
                            }
                            Some(Err(e)) => {
                                debug!("WebSocket error: {}", e);
                                break;
                            }
                            None => {
                                break;
                            }
                        }
                    }
                }
            }

            debug!("Reader task for {} exiting", connection_id);
        })
    }

    fn spawn_writer<Snk>(
        &self,
        connection_id: String,
        mut ws_sink: Snk,
        websocket_rx: flume::Receiver<String>,
        cancellation_token: CancellationToken,
    ) -> tokio::task::JoinHandle<()>
    where
        Snk: WsSink + Send + 'static,
    {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        let _ = ws_sink.send(WsMessage::Close(None)).await;
                        break;
                    }

                    message = websocket_rx.recv_async() => {
                        match message {
                            Ok(text) => {
                                // Message already converted to string by OutboundActor
                                if let Err(e) = ws_sink.send(WsMessage::Text(text)).await {
                                    error!("Failed to send message to WebSocket: {}", e);
                                    break;
                                }
                            }
                            Err(_) => {
                                // Channel closed
                                break;
                            }
                        }
                    }
                }
            }

            debug!("Writer task for {} exiting", connection_id);
        })
    }
}

/// Builder for the WebSocket handler
pub struct WebSocketBuilder<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    state_factory: F,
    middlewares: Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>,
    message_converter: C,
    channel_size: usize,
    max_connections: Option<usize>,
    max_connection_time: Option<Duration>,
}

impl<S, I, O, C, F> WebSocketBuilder<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    pub fn new(state_factory: F, message_converter: C) -> Self {
        Self {
            state_factory,
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

    pub fn build(self) -> WebSocketHandler<S, I, O, C, F> {
        WebSocketHandler::new(
            self.middlewares,
            self.message_converter,
            self.state_factory,
            self.channel_size,
            self.max_connections,
            self.max_connection_time,
        )
    }
}

/// Extension trait to add convenience methods for axum WebSocket
#[async_trait::async_trait]
pub trait AxumWebSocketExt<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    /// Start handling an axum WebSocket connection
    async fn start_axum(
        &self,
        socket: axum::extract::ws::WebSocket,
        connection_id: String,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl<S, I, O, C, F> AxumWebSocketExt<S, I, O, C, F> for WebSocketHandler<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    async fn start_axum(
        &self,
        socket: axum::extract::ws::WebSocket,
        connection_id: String,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<()> {
        let axum_ws = AxumWebSocket::new(socket);
        self.start(axum_ws, connection_id, cancellation_token).await
    }
}
