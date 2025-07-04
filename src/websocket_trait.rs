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
            WsMessage::Text(text) => axum::extract::ws::Message::Text(text.into()),
            WsMessage::Binary(data) => axum::extract::ws::Message::Binary(data.into()),
            WsMessage::Close(reason) => match reason {
                Some((code, reason)) => {
                    axum::extract::ws::Message::Close(Some(axum::extract::ws::CloseFrame {
                        code,
                        reason: reason.into(),
                    }))
                }
                None => axum::extract::ws::Message::Close(None),
            },
            WsMessage::Ping(data) => axum::extract::ws::Message::Ping(data.into()),
            WsMessage::Pong(data) => axum::extract::ws::Message::Pong(data.into()),
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
                        axum::extract::ws::Message::Text(text) => {
                            WsMessage::Text(text.as_str().to_owned())
                        }
                        axum::extract::ws::Message::Binary(data) => {
                            WsMessage::Binary(data.to_vec())
                        }
                        axum::extract::ws::Message::Close(frame) => {
                            let reason = frame.map(|f| (f.code, f.reason.as_str().to_owned()));
                            WsMessage::Close(reason)
                        }
                        axum::extract::ws::Message::Ping(data) => WsMessage::Ping(data.to_vec()),
                        axum::extract::ws::Message::Pong(data) => WsMessage::Pong(data.to_vec()),
                    };
                    Some(Ok(ws_msg))
                }
                Some(Err(e)) => Some(Err(WsError::WebSocket(e.to_string()))),
                None => None,
            }
        })
    }
}

/// FastWebSocket implementation
#[cfg(feature = "fastwebsockets")]
pub mod fast {
    use super::*;
    use fastwebsockets::{FragmentCollectorRead, Frame, OpCode, WebSocketError, WebSocketWrite};
    use tokio::io::{AsyncRead, AsyncWrite};

    /// FastWebSocket wrapper that implements WebSocketConnection
    pub struct FastWebSocket<S>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        ws: fastwebsockets::WebSocket<S>,
    }

    impl<S> FastWebSocket<S>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        pub fn new(ws: fastwebsockets::WebSocket<S>) -> Self {
            Self { ws }
        }
    }

    /// FastWebSocket sink wrapper
    pub struct FastWsSink<W>
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        writer: WebSocketWrite<W>,
    }

    /// FastWebSocket stream wrapper - contains the read half
    pub struct FastWsStream<S>
    where
        S: AsyncRead + Unpin + Send + 'static,
    {
        reader: FragmentCollectorRead<S>,
    }

    impl<S> WebSocketConnection for FastWebSocket<S>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        type Sink = FastWsSink<tokio::io::WriteHalf<S>>;
        type Stream = FastWsStream<tokio::io::ReadHalf<S>>;

        fn split(self) -> (Self::Sink, Self::Stream) {
            // Use tokio::io::split to split the underlying stream
            let (ws_read, ws_write) = self.ws.split(tokio::io::split);

            (
                FastWsSink { writer: ws_write },
                FastWsStream {
                    reader: FragmentCollectorRead::new(ws_read),
                },
            )
        }
    }

    #[async_trait]
    impl<W> WsSink for FastWsSink<W>
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
            let frame = match msg {
                WsMessage::Text(text) => Frame::text(text.into_bytes().into()),
                WsMessage::Binary(data) => Frame::binary(data.into()),
                WsMessage::Close(reason) => match reason {
                    Some((code, reason)) => Frame::close(code, reason.as_bytes()),
                    None => Frame::close(1000, &[]),
                },
                WsMessage::Ping(data) => Frame::new(true, OpCode::Ping, None, data.into()),
                WsMessage::Pong(data) => Frame::pong(data.into()),
            };

            self.writer
                .write_frame(frame)
                .await
                .map_err(|e| WsError::WebSocket(e.to_string()))
        }
    }

    impl<S> WsStream for FastWsStream<S>
    where
        S: AsyncRead + Unpin + Send + 'static,
    {
        fn next(&mut self) -> WsStreamFuture<'_> {
            Box::pin(async move {
                // FragmentCollectorRead requires a send function for obligated writes (pong/close)
                // Since we can't send from the read side, we'll use an empty closure
                match self
                    .reader
                    .read_frame(&mut |_frame| async { Ok::<(), WsError>(()) })
                    .await
                {
                    Ok(frame) => {
                        let ws_msg = match frame.opcode {
                            OpCode::Text => {
                                let text = String::from_utf8_lossy(&frame.payload).into_owned();
                                WsMessage::Text(text)
                            }
                            OpCode::Binary => WsMessage::Binary(frame.payload.to_vec()),
                            OpCode::Close => {
                                if frame.payload.len() >= 2 {
                                    let code =
                                        u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
                                    let reason =
                                        String::from_utf8_lossy(&frame.payload[2..]).into_owned();
                                    WsMessage::Close(Some((code, reason)))
                                } else {
                                    WsMessage::Close(None)
                                }
                            }
                            OpCode::Ping => WsMessage::Ping(frame.payload.to_vec()),
                            OpCode::Pong => WsMessage::Pong(frame.payload.to_vec()),
                            OpCode::Continuation => {
                                // This shouldn't happen with FragmentCollectorRead
                                return Some(Err(WsError::WebSocket(
                                    "Unexpected continuation frame".to_string(),
                                )));
                            }
                        };
                        Some(Ok(ws_msg))
                    }
                    Err(WebSocketError::ConnectionClosed) => None,
                    Err(e) => Some(Err(WsError::WebSocket(e.to_string()))),
                }
            })
        }
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
