use crate::Middleware;
use anyhow::Result;
use async_trait::async_trait;
use flume::{Sender, TrySendError};
use std::sync::Arc;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

/// A trait for converting between wire format (string) messages and application types.
///
/// This trait handles the conversion of messages between the wire format (strings)
/// used by the WebSocket protocol and the application-specific types used by
/// the middleware chain.
///
/// # Type Parameters
/// * `I` - The type of incoming messages after conversion
/// * `O` - The type of outgoing messages before conversion
pub trait MessageConverter<I, O>: Send + Sync {
    /// Converts an incoming string message to the application type.
    ///
    /// # Arguments
    /// * `message` - The string message received from the WebSocket
    ///
    /// # Returns
    /// * `Ok(Some(I))` - Successfully converted message
    /// * `Ok(None)` - Message should be ignored
    /// * `Err` - Conversion failed
    fn inbound_from_string(&self, message: String) -> Result<Option<I>, anyhow::Error>;

    /// Converts an outgoing application message to a string.
    ///
    /// # Arguments
    /// * `message` - The application message to convert
    ///
    /// # Returns
    /// * `Ok(String)` - Successfully converted message
    /// * `Err` - Conversion failed
    fn outbound_to_string(&self, message: O) -> Result<String, anyhow::Error>;
}

/// A wrapper for sending messages through the middleware chain.
///
/// This type provides a way to send messages back through the WebSocket connection
/// while maintaining the middleware chain's order and state.
///
/// # Type Parameters
/// * `O` - The type of outgoing messages
#[derive(Debug, Clone)]
pub struct MessageSender<O> {
    /// The channel sender for outgoing messages
    pub sender: Sender<(O, usize)>,
    /// The index of the middleware that sent the message
    pub index: usize,
}

impl<O> MessageSender<O> {
    /// Creates a new message sender.
    ///
    /// # Arguments
    /// * `sender` - The channel sender for outgoing messages
    /// * `index` - The index of the middleware that will use this sender
    pub fn new(sender: Sender<(O, usize)>, index: usize) -> Self {
        Self { sender, index }
    }

    /// Sends a message through the channel.
    ///
    /// This method attempts to send a message without blocking. If the channel
    /// is full, it will return an error immediately.
    ///
    /// # Arguments
    /// * `message` - The message to send
    ///
    /// # Returns
    /// * `Ok(())` - Message sent successfully
    /// * `Err(TrySendError)` - Channel is full or closed
    pub fn send(&mut self, message: O) -> Result<(), TrySendError<(O, usize)>> {
        debug!(
            "MessageSender sending message from middleware index: {}",
            self.index
        );

        if let Err(e) = self.sender.try_send((message, self.index)) {
            error!(
                "Failed to send message. Current capacity: {:?}. Error: {}",
                self.capacity(),
                e
            );
            return Err(e);
        }

        debug!("MessageSender successfully sent message");
        Ok(())
    }

    /// Sends a message through the channel, bypassing the current middleware.
    ///
    /// This method is useful when the current middleware has already processed
    /// the message (e.g., already applied filtering/validation) and you want to
    /// skip re-processing in the current middleware while still allowing
    /// other middleware in the chain to process the message.
    ///
    /// The outbound processing will start from the previous middleware in the chain
    /// (current index - 1), or index 0 if already at the first middleware.
    ///
    /// # When to use send_bypass() vs send():
    ///
    /// **Use `send_bypass()` when:**
    /// - You've already applied the current middleware's logic (e.g., filtering, validation)
    /// - You want to avoid duplicate processing in the current middleware
    /// - Example: Historical events that were already filtered during database query
    ///
    /// **Use `send()` when:**
    /// - You want the message to go through the normal middleware chain processing
    /// - The current middleware should also process this outbound message
    /// - Example: Broadcasting new events that need full filtering/validation
    ///
    /// # Arguments
    /// * `message` - The message to send
    ///
    /// # Returns
    /// * `Ok(())` - Message sent successfully
    /// * `Err(TrySendError)` - Channel is full or closed
    pub fn send_bypass(&mut self, message: O) -> Result<(), TrySendError<(O, usize)>> {
        let bypass_index = if self.index > 0 { self.index - 1 } else { 0 };

        debug!(
            "MessageSender bypassing current middleware (index {}) and starting from index {}",
            self.index, bypass_index
        );

        if let Err(e) = self.sender.try_send((message, bypass_index)) {
            error!(
                "Failed to send bypass message. Current capacity: {:?}. Error: {}",
                self.capacity(),
                e
            );
            return Err(e);
        }

        debug!("MessageSender successfully sent bypass message");
        Ok(())
    }

    /// Returns the number of available slots in the channel.
    pub fn capacity(&self) -> Option<usize> {
        self.sender.capacity()
    }
}

/// A trait for sending messages through the middleware chain.
///
/// This trait provides a common interface for sending messages back through
/// the WebSocket connection, regardless of the context type.
///
/// # Type Parameters
/// * `O` - The type of outgoing messages
#[async_trait]
pub trait SendMessage<O> {
    /// Queue `message` for the client via the bounded, non-blocking channel.
    ///
    /// • Non-blocking (`try_send`) → avoids dead-locks
    /// • Preserves outbound-middleware order (continues after current middleware)
    ///
    /// Returns `Err(TrySendError::Full | Closed)` as the back-pressure signal.
    fn send_message(&mut self, message: O) -> Result<()>;

    /// Returns the number of available slots in the channel.
    fn capacity(&self) -> Option<usize>;
}

/// Context for handling connection establishment.
#[derive(Debug)]
pub struct ConnectionContext<S, M, O>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    pub connection_id: String,
    pub state: Arc<tokio::sync::RwLock<S>>,
    pub sender: Option<MessageSender<O>>,
    pub(crate) index: usize,
    pub(crate) middlewares: SharedMiddlewareVec<S, M, O>,
}

impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static>
    ConnectionContext<S, M, O>
{
    pub fn new(
        connection_id: String,
        sender: Option<Sender<(O, usize)>>,
        state: Arc<tokio::sync::RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        index: usize,
    ) -> Self {
        Self {
            connection_id,
            sender: sender.map(|s| MessageSender::new(s, index)),
            state,
            middlewares,
            index,
        }
    }

    pub async fn next(&mut self) -> Result<()> {
        if self.index >= self.middlewares.len() - 1 {
            return Ok(());
        }
        self.index += 1;
        if let Some(sender) = &mut self.sender {
            sender.index = self.index;
        }
        let middleware = self.middlewares[self.index].clone();
        middleware.on_connect(self).await
    }
}

#[async_trait]
impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static> SendMessage<O>
    for ConnectionContext<S, M, O>
{
    fn send_message(&mut self, message: O) -> Result<()> {
        if let Some(sender) = &mut self.sender {
            sender.send(message)?;
        }
        Ok(())
    }

    fn capacity(&self) -> Option<usize> {
        self.sender.as_ref().and_then(|s| s.capacity())
    }
}

/// Context for handling connection termination.
#[derive(Debug)]
pub struct DisconnectContext<S, M, O>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    pub connection_id: String,
    pub state: Arc<tokio::sync::RwLock<S>>,
    pub sender: Option<MessageSender<O>>,
    pub(crate) index: usize,
    pub(crate) middlewares: SharedMiddlewareVec<S, M, O>,
}

impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static>
    DisconnectContext<S, M, O>
{
    pub fn new(
        connection_id: String,
        sender: Option<Sender<(O, usize)>>,
        state: Arc<tokio::sync::RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        index: usize,
    ) -> Self {
        Self {
            connection_id,
            sender: sender.map(|s| MessageSender::new(s, index)),
            state,
            middlewares,
            index,
        }
    }

    pub async fn next(&mut self) -> Result<()> {
        if self.index >= self.middlewares.len() - 1 {
            return Ok(());
        }
        self.index += 1;
        if let Some(sender) = &mut self.sender {
            sender.index = self.index;
        }
        let middleware = self.middlewares[self.index].clone();
        middleware.on_disconnect(self).await
    }
}

#[async_trait]
impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static> SendMessage<O>
    for DisconnectContext<S, M, O>
{
    fn send_message(&mut self, message: O) -> Result<()> {
        if let Some(sender) = &mut self.sender {
            sender.send(message)?;
        }
        Ok(())
    }

    fn capacity(&self) -> Option<usize> {
        self.sender.as_ref().and_then(|s| s.capacity())
    }
}

/// Context for handling incoming messages.
#[derive(Debug)]
pub struct InboundContext<S, M, O>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    pub connection_id: String,
    pub state: Arc<tokio::sync::RwLock<S>>,
    pub sender: Option<MessageSender<O>>,
    pub(crate) index: usize,
    pub(crate) middlewares: SharedMiddlewareVec<S, M, O>,
    pub message: Option<M>,
}

impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static>
    InboundContext<S, M, O>
{
    pub fn new(
        connection_id: String,
        message: Option<M>,
        sender: Option<Sender<(O, usize)>>,
        state: Arc<tokio::sync::RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        index: usize,
    ) -> Self {
        Self {
            connection_id,
            sender: sender.map(|s| MessageSender::new(s, index)),
            state,
            middlewares,
            index,
            message,
        }
    }

    pub async fn next(&mut self) -> Result<()> {
        if self.message.is_none() {
            debug!("Inbound message is empty, stopping middleware chain");
            return Ok(());
        }
        if self.index >= self.middlewares.len() - 1 {
            return Ok(());
        }
        self.index += 1;
        if let Some(sender) = &mut self.sender {
            sender.index = self.index;
        }
        let middleware = self.middlewares[self.index].clone();
        middleware.process_inbound(self).await
    }
}

#[async_trait]
impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static> SendMessage<O>
    for InboundContext<S, M, O>
{
    fn send_message(&mut self, message: O) -> Result<()> {
        if let Some(sender) = &mut self.sender {
            sender.send(message)?;
        }
        Ok(())
    }

    fn capacity(&self) -> Option<usize> {
        self.sender.as_ref().and_then(|s| s.capacity())
    }
}

/// Context for handling outgoing messages.
#[derive(Debug)]
pub struct OutboundContext<S, M, O>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    pub connection_id: String,
    pub state: Arc<tokio::sync::RwLock<S>>,
    pub sender: Option<MessageSender<O>>,
    pub(crate) index: usize,
    pub(crate) middlewares: SharedMiddlewareVec<S, M, O>,
    pub message: Option<O>,
}

impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static>
    OutboundContext<S, M, O>
{
    pub fn new(
        connection_id: String,
        message: O,
        sender: Option<Sender<(O, usize)>>,
        state: Arc<tokio::sync::RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        index: usize,
    ) -> Self {
        Self {
            connection_id,
            sender: sender.map(|s| MessageSender::new(s, index)),
            state,
            middlewares,
            index,
            message: Some(message),
        }
    }

    pub async fn next(&mut self) -> Result<()> {
        if self.index == 0 {
            return Ok(());
        }
        self.index -= 1;
        if let Some(sender) = &mut self.sender {
            sender.index = self.index;
        }
        let middleware = self.middlewares[self.index].clone();
        middleware.process_outbound(self).await
    }
}

#[async_trait]
impl<S: Send + Sync + 'static, M: Send + Sync + 'static, O: Send + Sync + 'static> SendMessage<O>
    for OutboundContext<S, M, O>
{
    fn send_message(&mut self, message: O) -> Result<()> {
        if let Some(sender) = &mut self.sender {
            sender.send(message)?;
        }
        Ok(())
    }

    fn capacity(&self) -> Option<usize> {
        self.sender.as_ref().and_then(|s| s.capacity())
    }
}

/// Factory trait for creating WebSocket connection state.
///
/// This trait defines how to create initial state for each WebSocket connection.
/// Each connection gets its own state instance that can be used to store
/// connection-specific data throughout the middleware chain.
pub trait StateFactory<State> {
    /// Creates a new state instance for each WebSocket connection.
    ///
    /// This method is called when a new WebSocket connection is established.
    /// The returned state instance will be passed through the middleware chain
    /// and can be used to store connection-specific data.
    ///
    /// # Arguments
    /// * `token` - A cancellation token that will be cancelled when the connection ends.
    ///   This token can be used to clean up resources when the connection is closed.
    fn create_state(&self, token: CancellationToken) -> State;
}

/// Type alias for a collection of middleware.
///
/// # Type Parameters
/// * `S` - The type of state maintained for each connection
/// * `I` - The type of incoming messages
/// * `O` - The type of outgoing messages
pub type MiddlewareVec<S, I, O> =
    Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>;

/// Type alias for a shared collection of middleware.
///
/// This is used in contexts where middleware collections need to be shared
/// across actors or threads.
///
/// # Type Parameters
/// * `S` - The type of state maintained for each connection
/// * `I` - The type of incoming messages
/// * `O` - The type of outgoing messages
pub type SharedMiddlewareVec<S, I, O> =
    Arc<Vec<Arc<dyn Middleware<State = S, IncomingMessage = I, OutgoingMessage = O>>>>;

/// Error type for WebSocket operations.
///
/// This error type encapsulates various error conditions that can occur during
/// WebSocket connection handling and maintains the connection state for proper
/// cleanup and error recovery.
///
/// # Type Parameters
/// * `ConnectionState` - The type of state maintained for each connection
#[derive(Error, Debug)]
pub enum WebsocketError<ConnectionState: Send + Sync + 'static> {
    #[error("IO error: {0}")]
    IoError(std::io::Error, ConnectionState),

    #[error("Invalid target URL: missing host")]
    InvalidTargetUrl(ConnectionState),

    #[error("Inbound message conversion error: {0}")]
    InboundMessageConversionError(anyhow::Error, ConnectionState),

    #[error("Outbound message conversion error: {0}")]
    OutboundMessageConversionError(anyhow::Error, ConnectionState),

    #[error("Handler error: {0}")]
    HandlerError(anyhow::Error, ConnectionState),

    #[error("Missing middleware")]
    MissingMiddleware(ConnectionState),

    #[error("Maximum connections exceeded")]
    MaxConnectionsExceeded(ConnectionState),

    #[error("Task join error: {0}")]
    JoinError(tokio::task::JoinError, ConnectionState),

    #[error("WebSocket error: {0}")]
    WebsocketError(anyhow::Error, ConnectionState),

    #[error("No closing handshake")]
    NoClosingHandshake(anyhow::Error, ConnectionState),

    #[error("Binary messages are not supported by this server")]
    UnsupportedBinaryMessage(ConnectionState),
}

impl<ConnectionState: Send + Sync + 'static> WebsocketError<ConnectionState> {
    pub fn get_state(self) -> ConnectionState {
        match self {
            Self::HandlerError(_, state) => state,
            Self::IoError(_, state) => state,
            Self::InvalidTargetUrl(state) => state,
            Self::InboundMessageConversionError(_, state) => state,
            Self::OutboundMessageConversionError(_, state) => state,
            Self::JoinError(_, state) => state,
            Self::WebsocketError(_, state) => state,
            Self::NoClosingHandshake(_, state) => state,
            Self::MissingMiddleware(state) => state,
            Self::MaxConnectionsExceeded(state) => state,
            Self::UnsupportedBinaryMessage(state) => state,
        }
    }
}
