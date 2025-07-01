use anyhow::Result;
use flume::{Receiver, Sender};
use parking_lot::RwLock;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    ConnectionContext, DisconnectContext, InboundContext, MessageConverter, OutboundContext,
    SharedMiddlewareVec, WsMessage, WsSink, WsStream,
};

/// Configuration for InboundActor
pub struct InboundActorConfig<S, M, O> {
    pub state: Arc<RwLock<S>>,
    pub middlewares: SharedMiddlewareVec<S, M, O>,
    pub outbound_sender: Sender<(O, usize)>,
    pub control_sender: Sender<WsMessage>,
    pub message_converter: Arc<dyn MessageConverter<M, O>>,
    pub connection_id: String,
    pub cancellation_token: CancellationToken,
}

/// Actor responsible for processing inbound messages
pub struct InboundActor<S, M, O, Str>
where
    Str: WsStream,
{
    state: Arc<RwLock<S>>,
    middlewares: SharedMiddlewareVec<S, M, O>,
    ws_stream: Str,
    outbound_sender: Sender<(O, usize)>,
    control_sender: Sender<WsMessage>,
    message_converter: Arc<dyn MessageConverter<M, O>>,
    connection_id: String,
    cancellation_token: CancellationToken,
}

impl<S, M, O, Str> InboundActor<S, M, O, Str>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
    Str: WsStream + Send + 'static,
{
    /// Creates a new InboundActor
    pub fn new(config: InboundActorConfig<S, M, O>, ws_stream: Str) -> Self {
        Self {
            state: config.state,
            middlewares: config.middlewares,
            ws_stream,
            outbound_sender: config.outbound_sender,
            control_sender: config.control_sender,
            message_converter: config.message_converter,
            connection_id: config.connection_id,
            cancellation_token: config.cancellation_token,
        }
    }

    /// Spawns the actor
    pub fn spawn(config: InboundActorConfig<S, M, O>, ws_stream: Str) {
        let connection_id = config.connection_id.clone();
        let mut actor = Self::new(config, ws_stream);

        tokio::spawn(async move {
            debug!("InboundActor starting for connection {}", connection_id);
            actor.run().await;

            // Ensure cancellation is triggered when actor stops for any reason
            if !actor.cancellation_token.is_cancelled() {
                debug!(
                    "InboundActor triggering cancellation on exit for connection {}",
                    connection_id
                );
                actor.cancellation_token.cancel();
            }

            info!("InboundActor stopped for connection {}", connection_id);
        });
    }

    /// Main run loop for the actor
    async fn run(&mut self) {
        loop {
            select! {
                _ = self.cancellation_token.cancelled() => {
                    debug!("InboundActor cancelled for connection {}", self.connection_id);
                    break;
                }

                message = self.ws_stream.next() => {
                    match message {
                        Some(Ok(WsMessage::Text(text))) => {
                            match self.message_converter.inbound_from_string(text) {
                                Ok(Some(msg)) => {
                                    // Process directly
                                    if let Err(e) = self.process_inbound(msg).await {
                                        error!("Error processing inbound message: {}", e);
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
                            error!("Protocol violation: received binary message with {} bytes - terminating connection", data.len());
                            break;
                        }
                        Some(Ok(WsMessage::Ping(data))) => {
                            debug!("Received ping with {} bytes", data.len());
                            // Send pong response with the same data
                            if let Err(e) = self.control_sender.try_send(WsMessage::Pong(data)) {
                                debug!("Failed to send pong response: {}", e);
                            }
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
                            debug!("WebSocket stream ended");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Process a single inbound message through the middleware chain
    async fn process_inbound(&mut self, message: M) -> Result<()> {
        debug!(
            "Processing inbound message for connection {}",
            self.connection_id
        );

        let mut ctx = InboundContext::new(
            self.connection_id.clone(),
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

/// Configuration for OutboundActor
pub struct OutboundActorConfig<S, M, O> {
    pub state: Arc<RwLock<S>>,
    pub middlewares: SharedMiddlewareVec<S, M, O>,
    pub connection_id: String,
    pub message_converter: Arc<dyn MessageConverter<M, O>>,
    pub cancellation_token: CancellationToken,
    pub channel_size: usize,
}

/// Actor responsible for processing outbound messages
pub struct OutboundActor<S, M, O, Snk>
where
    Snk: WsSink,
{
    state: Arc<RwLock<S>>,
    middlewares: SharedMiddlewareVec<S, M, O>,
    receiver: Receiver<(O, usize)>, // (message, middleware_index)
    control_receiver: Receiver<WsMessage>, // Control frames (ping/pong)
    connection_id: String,
    /// Direct WebSocket sink
    ws_sink: Snk,
    /// Converter to transform messages to strings
    message_converter: Arc<dyn MessageConverter<M, O>>,
    /// Cancellation token for graceful shutdown
    cancellation_token: CancellationToken,
}

impl<S, M, O, Snk> OutboundActor<S, M, O, Snk>
where
    S: Send + Sync + 'static,
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
    Snk: WsSink + Send + 'static,
{
    /// Creates a new OutboundActor
    fn new(
        config: OutboundActorConfig<S, M, O>,
        ws_sink: Snk,
    ) -> (Self, Sender<(O, usize)>, Sender<WsMessage>) {
        let (tx, rx) = flume::bounded::<(O, usize)>(config.channel_size);
        let (control_tx, control_rx) = flume::bounded::<WsMessage>(32); // Small buffer for control frames

        let actor = Self {
            state: config.state,
            middlewares: config.middlewares,
            receiver: rx,
            control_receiver: control_rx,
            connection_id: config.connection_id,
            ws_sink,
            message_converter: config.message_converter,
            cancellation_token: config.cancellation_token,
        };

        (actor, tx, control_tx)
    }

    /// Spawns the actor and returns senders for outbound messages and control frames
    pub fn spawn(
        config: OutboundActorConfig<S, M, O>,
        ws_sink: Snk,
    ) -> (Sender<(O, usize)>, Sender<WsMessage>) {
        let connection_id = config.connection_id.clone();
        let (mut actor, tx, control_tx) = Self::new(config, ws_sink);

        tokio::spawn(async move {
            debug!("OutboundActor starting for connection {}", connection_id);
            actor.run().await;

            // Ensure cancellation is triggered when actor stops for any reason
            if !actor.cancellation_token.is_cancelled() {
                debug!(
                    "OutboundActor triggering cancellation on exit for connection {}",
                    connection_id
                );
                actor.cancellation_token.cancel();
            }

            debug!("OutboundActor stopped for connection {}", connection_id);
        });

        (tx, control_tx)
    }

    /// Main run loop for the actor
    async fn run(&mut self) {
        loop {
            select! {
                _ = self.cancellation_token.cancelled() => {
                    // Send close frame before shutting down
                    debug!("OutboundActor cancelled, sending close frame");
                    let _ = self.ws_sink.send(WsMessage::Close(None)).await;

                    // Drain any remaining messages without processing them
                    while self.receiver.try_recv().is_ok() {
                        debug!("Discarding message after cancellation");
                    }
                    while self.control_receiver.try_recv().is_ok() {
                        debug!("Discarding control frame after cancellation");
                    }

                    break;
                }

                control_frame = self.control_receiver.recv_async() => {
                    match control_frame {
                        Ok(frame) => {
                            // Check if we've been cancelled before sending
                            if self.cancellation_token.is_cancelled() {
                                debug!("Discarding control frame - connection cancelled");
                                continue;
                            }

                            // Send control frame directly
                            if let Err(e) = self.ws_sink.send(frame).await {
                                if !self.cancellation_token.is_cancelled() {
                                    error!("Failed to send control frame: {}", e);
                                }
                            }
                        }
                        Err(_) => {
                            // Control channel closed
                            debug!("OutboundActor control receiver closed");
                        }
                    }
                }

                message = self.receiver.recv_async() => {
                    match message {
                        Ok((msg, middleware_index)) => {
                            // Check if we've been cancelled before processing
                            if self.cancellation_token.is_cancelled() {
                                debug!("Discarding message - connection cancelled");
                                continue;
                            }

                            if let Err(e) = self.process_outbound(msg, middleware_index).await {
                                error!("Error processing outbound message: {}", e);
                            }
                        }
                        Err(_) => {
                            // Channel closed
                            debug!("OutboundActor receiver closed");
                            break;
                        }
                    }
                }
            }
        }
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

            // Send with timeout to prevent blocking forever
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                self.ws_sink.send(WsMessage::Text(string_message)),
            )
            .await
            {
                Ok(Ok(())) => {
                    // Success
                }
                Ok(Err(e)) => {
                    let error_string = e.to_string();

                    // Check for the specific close-related error
                    if error_string.contains("Sending after closing is not allowed") {
                        // This is expected during close handshake, but we want to track what was dropped
                        warn!(
                            "Message dropped during close handshake for connection {}",
                            self.connection_id
                        );
                        return Ok(());
                    }

                    // Only log error if we're not cancelled (expected during shutdown)
                    if !self.cancellation_token.is_cancelled() {
                        error!("Failed to send message to WebSocket: {}", e);
                    }
                    return Err(anyhow::anyhow!("WebSocket send failed: {}", e));
                }
                Err(_) => {
                    error!("Timeout sending message to WebSocket");
                    return Err(anyhow::anyhow!("WebSocket send timeout"));
                }
            }
        }

        Ok(())
    }
}

/// Configuration for spawning split actors
pub struct SplitActorsConfig<S, M, O> {
    pub state: Arc<RwLock<S>>,
    pub middlewares: SharedMiddlewareVec<S, M, O>,
    pub connection_id: String,
    pub channel_size: usize,
    pub message_converter: Arc<dyn MessageConverter<M, O>>,
    pub cancellation_token: CancellationToken,
}

/// Helper struct to manage the lifecycle of both actors
pub struct SplitActors<M, O> {
    _m: PhantomData<M>,
    _o: PhantomData<O>,
}

impl<M, O> SplitActors<M, O>
where
    M: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    /// Create and spawn both actors with direct WebSocket I/O
    pub fn spawn<S, Str, Snk>(
        config: SplitActorsConfig<S, M, O>,
        ws_stream: Str,
        ws_sink: Snk,
    ) -> Sender<(O, usize)>
    where
        S: Send + Sync + 'static,
        Str: WsStream + Send + 'static,
        Snk: WsSink + Send + 'static,
    {
        // Create outbound actor config
        let outbound_config = OutboundActorConfig {
            state: config.state.clone(),
            middlewares: config.middlewares.clone(),
            connection_id: config.connection_id.clone(),
            message_converter: config.message_converter.clone(),
            cancellation_token: config.cancellation_token.clone(),
            channel_size: config.channel_size,
        };

        // Create outbound actor first
        let (outbound_sender, control_sender) = OutboundActor::spawn(outbound_config, ws_sink);

        // Create inbound actor config
        let inbound_config = InboundActorConfig {
            state: config.state,
            middlewares: config.middlewares,
            outbound_sender: outbound_sender.clone(),
            control_sender,
            message_converter: config.message_converter,
            connection_id: config.connection_id,
            cancellation_token: config.cancellation_token,
        };

        // Create inbound actor with direct stream reading
        InboundActor::spawn(inbound_config, ws_stream);

        // Return only the outbound sender since inbound reads directly from stream
        outbound_sender
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

    // Only call the first middleware - it will chain to the rest via ctx.next()
    if !middlewares.is_empty() {
        let mut ctx = ConnectionContext::new(
            connection_id.to_string(),
            Some(outbound_sender.clone()),
            state.clone(),
            middlewares.clone(),
            0, // Start at index 0
        );

        middlewares[0].on_connect(&mut ctx).await?;
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

    // Only call the first middleware - it will chain to the rest via ctx.next()
    if !middlewares.is_empty() {
        let mut ctx = DisconnectContext::new(
            connection_id.to_string(),
            Some(outbound_sender.clone()),
            state.clone(),
            middlewares.clone(),
            0, // Start at index 0
        );

        middlewares[0].on_disconnect(&mut ctx).await?;
    }

    Ok(())
}
