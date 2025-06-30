//! WebSocket trait abstraction for framework independence
//!
//! This module provides traits that abstract over WebSocket implementations,
//! allowing the actor handler to work with different WebSocket frameworks.

use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::pin::Pin;

/// Represents a WebSocket message
#[derive(Debug, Clone)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Close(Option<(u16, String)>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
}

/// Error type for WebSocket operations
#[derive(Debug, thiserror::Error)]
pub enum WsError {
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("Connection closed")]
    ConnectionClosed,
}

/// Trait for WebSocket sink (sending messages)
#[async_trait]
pub trait WsSink: Send + Unpin {
    async fn send(&mut self, msg: WsMessage) -> Result<(), WsError>;
}

/// Future type for stream next operation
pub type WsStreamFuture<'a> =
    Pin<Box<dyn std::future::Future<Output = Option<Result<WsMessage, WsError>>> + Send + 'a>>;

/// Trait for WebSocket stream (receiving messages)
pub trait WsStream: Send + Unpin {
    fn next(&mut self) -> WsStreamFuture<'_>;
}

/// Trait for a WebSocket connection that can be split
pub trait WebSocketConnection: Send {
    type Sink: WsSink;
    type Stream: WsStream;

    fn split(self) -> (Self::Sink, Self::Stream);
}

/// Axum WebSocket implementation
pub struct AxumWebSocket {
    socket: axum::extract::ws::WebSocket,
}

impl AxumWebSocket {
    pub fn new(socket: axum::extract::ws::WebSocket) -> Self {
        Self { socket }
    }
}

/// Axum WebSocket sink wrapper
pub struct AxumWsSink {
    sink: SplitSink<axum::extract::ws::WebSocket, axum::extract::ws::Message>,
}

/// Axum WebSocket stream wrapper
pub struct AxumWsStream {
    stream: SplitStream<axum::extract::ws::WebSocket>,
}

impl WebSocketConnection for AxumWebSocket {
    type Sink = AxumWsSink;
    type Stream = AxumWsStream;

    fn split(self) -> (Self::Sink, Self::Stream) {
        let (sink, stream) = self.socket.split();
        (AxumWsSink { sink }, AxumWsStream { stream })
    }
}

#[async_trait]
impl WsSink for AxumWsSink {
    async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        let axum_msg = match msg {
            WsMessage::Text(text) => axum::extract::ws::Message::Text(text),
            WsMessage::Binary(data) => axum::extract::ws::Message::Binary(data),
            WsMessage::Close(reason) => match reason {
                Some((code, reason)) => {
                    axum::extract::ws::Message::Close(Some(axum::extract::ws::CloseFrame {
                        code,
                        reason: std::borrow::Cow::Owned(reason),
                    }))
                }
                None => axum::extract::ws::Message::Close(None),
            },
            WsMessage::Ping(data) => axum::extract::ws::Message::Ping(data),
            WsMessage::Pong(data) => axum::extract::ws::Message::Pong(data),
        };

        self.sink
            .send(axum_msg)
            .await
            .map_err(|e| WsError::WebSocket(e.to_string()))
    }
}

impl WsStream for AxumWsStream {
    fn next(&mut self) -> WsStreamFuture<'_> {
        Box::pin(async move {
            match self.stream.next().await {
                Some(Ok(msg)) => {
                    let ws_msg = match msg {
                        axum::extract::ws::Message::Text(text) => WsMessage::Text(text),
                        axum::extract::ws::Message::Binary(data) => WsMessage::Binary(data),
                        axum::extract::ws::Message::Close(frame) => {
                            let reason = frame.map(|f| (f.code, f.reason.to_string()));
                            WsMessage::Close(reason)
                        }
                        axum::extract::ws::Message::Ping(data) => WsMessage::Ping(data),
                        axum::extract::ws::Message::Pong(data) => WsMessage::Pong(data),
                    };
                    Some(Ok(ws_msg))
                }
                Some(Err(e)) => Some(Err(WsError::WebSocket(e.to_string()))),
                None => None,
            }
        })
    }
}

// Mock WebSocket implementations for testing
#[cfg(test)]
pub mod mock {
    use super::*;
    use tokio::sync::mpsc;

    /// Create a mock WebSocket pair for testing
    /// Returns (sink, stream, sink_rx, stream_tx) where:
    /// - sink: The mock sink that the actor will write to
    /// - stream: The mock stream that the actor will read from
    /// - sink_rx: Receive messages that were sent to the sink
    /// - stream_tx: Send messages that will be read from the stream
    pub fn create_mock_websocket() -> (
        MockWsSink,
        MockWsStream,
        mpsc::Receiver<WsMessage>,
        mpsc::Sender<Result<WsMessage, WsError>>,
    ) {
        let (sink_tx, sink_rx) = mpsc::channel(100);
        let (stream_tx, stream_rx) = mpsc::channel(100);

        (
            MockWsSink { tx: sink_tx },
            MockWsStream { rx: stream_rx },
            sink_rx,
            stream_tx,
        )
    }

    /// Mock WebSocket sink
    pub struct MockWsSink {
        tx: mpsc::Sender<WsMessage>,
    }

    /// Mock WebSocket stream  
    pub struct MockWsStream {
        rx: mpsc::Receiver<Result<WsMessage, WsError>>,
    }

    #[async_trait]
    impl WsSink for MockWsSink {
        async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
            self.tx
                .send(msg)
                .await
                .map_err(|_| WsError::ConnectionClosed)
        }
    }

    impl WsStream for MockWsStream {
        fn next(&mut self) -> WsStreamFuture<'_> {
            Box::pin(async move { self.rx.recv().await })
        }
    }
}
