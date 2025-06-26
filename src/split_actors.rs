use anyhow::Result;
use flume::{Receiver, Sender};
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::{select, sync::RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::{
    ConnectionContext, DisconnectContext, InboundContext, OutboundContext, SharedMiddlewareVec,
};

/// Actor responsible for processing inbound messages
pub struct InboundActor<S, M, O> {
    state: Arc<RwLock<S>>,
    middlewares: SharedMiddlewareVec<S, M, O>,
    receiver: Receiver<(String, M)>, // (connection_id, message)
    outbound_sender: Sender<(O, usize)>,
    _connection_id: String,
}

impl<S, M, O> InboundActor<S, M, O>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    /// Creates a new InboundActor
    pub fn new(
        state: Arc<RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        receiver: Receiver<(String, M)>,
        outbound_sender: Sender<(O, usize)>,
        connection_id: String,
    ) -> Self {
        Self {
            state,
            middlewares,
            receiver,
            outbound_sender,
            _connection_id: connection_id,
        }
    }

    /// Spawns the actor and returns a sender for inbound messages
    pub fn spawn(
        state: Arc<RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        outbound_sender: Sender<(O, usize)>,
        connection_id: String,
        channel_size: usize,
    ) -> Sender<(String, M)> {
        let (tx, rx) = flume::bounded::<(String, M)>(channel_size);

        let mut actor = Self::new(
            state,
            middlewares,
            rx,
            outbound_sender,
            connection_id.clone(),
        );

        tokio::spawn(async move {
            debug!("InboundActor starting for connection {}", connection_id);
            actor.run().await;
            info!("InboundActor stopped for connection {}", connection_id);
        });

        tx
    }

    /// Main run loop for the actor
    async fn run(&mut self) {
        while let Ok((connection_id, message)) = self.receiver.recv_async().await {
            if let Err(e) = self.process_inbound(connection_id, message).await {
                error!("Error processing inbound message: {}", e);
            }
        }
        debug!("InboundActor receiver closed, shutting down");
    }

    /// Process a single inbound message through the middleware chain
    async fn process_inbound(&mut self, connection_id: String, message: M) -> Result<()> {
        debug!(
            "Processing inbound message for connection {}",
            connection_id
        );

        let mut ctx = InboundContext::new(
            connection_id,
            Some(message),
            Some(self.outbound_sender.clone()),
            self.state.clone(),
            self.middlewares.clone(),
            0,
        );

        // Process through middleware chain
        if let Some(middleware) = self.middlewares.first() {
            middleware.process_inbound(&mut ctx).await?;
        }

        Ok(())
    }
}

/// Actor responsible for processing outbound messages
pub struct OutboundActor<S, M, O> {
    state: Arc<RwLock<S>>,
    middlewares: SharedMiddlewareVec<S, M, O>,
    receiver: Receiver<(O, usize)>, // (message, middleware_index)
    connection_id: String,
    /// Channel to send final messages to the WebSocket writer
    websocket_sender: Sender<String>,
    /// Converter to transform messages to strings
    message_converter: Arc<dyn crate::MessageConverter<M, O>>,
}

impl<S, M, O> OutboundActor<S, M, O>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    /// Creates a new OutboundActor
    fn new(
        state: Arc<RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        receiver: Receiver<(O, usize)>,
        connection_id: String,
        websocket_sender: Sender<String>,
        message_converter: Arc<dyn crate::MessageConverter<M, O>>,
    ) -> Self {
        Self {
            state,
            middlewares,
            receiver,
            connection_id,
            websocket_sender,
            message_converter,
        }
    }

    /// Spawns the actor and returns a sender for outbound messages
    pub fn spawn(
        state: Arc<RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        connection_id: String,
        channel_size: usize,
        websocket_sender: Sender<String>,
        message_converter: Arc<dyn crate::MessageConverter<M, O>>,
        cancellation_token: CancellationToken,
    ) -> Sender<(O, usize)> {
        let (tx, rx) = flume::bounded::<(O, usize)>(channel_size);

        let mut actor = Self::new(
            state,
            middlewares,
            rx,
            connection_id.clone(),
            websocket_sender,
            message_converter,
        );

        tokio::spawn(async move {
            debug!("OutboundActor starting for connection {}", connection_id);
            loop {
                select! {
                    Ok((message, middleware_index)) = actor.receiver.recv_async() => {
                        if let Err(e) = actor.process_outbound(message, middleware_index).await {
                            error!("Error processing outbound message: {}", e);
                        }
                    }
                    _ = cancellation_token.cancelled() => {
                        break;
                    }
                }
            }
            debug!("OutboundActor receiver closed, shutting down");
        });

        tx
    }

    /// Process a single outbound message through the middleware chain
    async fn process_outbound(&mut self, message: O, middleware_index: usize) -> Result<()> {
        debug!(
            "Processing outbound message for connection {} starting at middleware {}",
            self.connection_id, middleware_index
        );

        let mut ctx = OutboundContext::new(
            self.connection_id.clone(),
            message,
            None, // OutboundActor doesn't send to itself
            self.state.clone(),
            self.middlewares.clone(),
            middleware_index,
        );

        // Process through middleware chain starting at the given index
        if middleware_index < self.middlewares.len() {
            self.middlewares[middleware_index]
                .process_outbound(&mut ctx)
                .await?;
        }

        // If message made it through all middleware, send to WebSocket
        if let Some(final_message) = ctx.message {
            let string_message = self.message_converter.outbound_to_string(final_message)?;
            if let Err(e) = self.websocket_sender.try_send(string_message) {
                error!("Failed to send message to WebSocket writer: {}", e);
            }
        }

        Ok(())
    }
}

/// Helper struct to manage the lifecycle of both actors
pub struct SplitActors<M, O> {
    _m: PhantomData<M>,
    _o: PhantomData<O>,
}

/// Type alias for the actor senders
type ActorSenders<M, O> = (Sender<(String, M)>, Sender<(O, usize)>);

impl<M, O> SplitActors<M, O>
where
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    /// Create and spawn both actors
    pub fn spawn<S>(
        state: Arc<RwLock<S>>,
        middlewares: SharedMiddlewareVec<S, M, O>,
        connection_id: String,
        channel_size: usize,
        websocket_sender: Sender<String>,
        message_converter: Arc<dyn crate::MessageConverter<M, O>>,
        cancellation_token: CancellationToken,
    ) -> ActorSenders<M, O>
    where
        S: Send + Sync + 'static,
    {
        // Create outbound actor first since inbound needs its sender
        let outbound_sender = OutboundActor::spawn(
            state.clone(),
            middlewares.clone(),
            connection_id.clone(),
            channel_size,
            websocket_sender.clone(),
            message_converter,
            cancellation_token,
        );

        // Create inbound actor
        let inbound_sender = InboundActor::spawn(
            state.clone(),
            middlewares,
            outbound_sender.clone(),
            connection_id,
            channel_size,
        );

        (inbound_sender, outbound_sender)
    }
}

/// Process on_connect event directly without actors
pub async fn process_on_connect<S, M, O>(
    state: Arc<RwLock<S>>,
    middlewares: &SharedMiddlewareVec<S, M, O>,
    connection_id: &str,
    outbound_sender: &Sender<(O, usize)>,
) -> Result<()>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    debug!("Processing on_connect for connection {}", connection_id);

    for (index, middleware) in middlewares.iter().enumerate() {
        let mut ctx = ConnectionContext::new(
            connection_id.to_string(),
            Some(outbound_sender.clone()),
            state.clone(),
            middlewares.clone(),
            index,
        );

        middleware.on_connect(&mut ctx).await?;
    }

    Ok(())
}

/// Process on_disconnect event directly without actors
pub async fn process_on_disconnect<S, M, O>(
    state: Arc<RwLock<S>>,
    middlewares: &SharedMiddlewareVec<S, M, O>,
    connection_id: &str,
    outbound_sender: Sender<(O, usize)>,
) -> Result<()>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    debug!("Processing on_disconnect for connection {}", connection_id);

    for (index, middleware) in middlewares.iter().enumerate() {
        let mut ctx = DisconnectContext::new(
            connection_id.to_string(),
            Some(outbound_sender.clone()),
            state.clone(),
            middlewares.clone(),
            index,
        );

        middleware.on_disconnect(&mut ctx).await?;
    }

    Ok(())
}
