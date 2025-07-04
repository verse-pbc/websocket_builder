//! Unified WebSocket API that works with both tungstenite and fastwebsockets
//!
//! This module provides a consistent interface regardless of which WebSocket
//! implementation is being used.

use crate::{MessageConverter, StateFactory, WebSocketHandler};
use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::request::Parts,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[cfg(not(any(feature = "tungstenite", feature = "fastwebsockets")))]
compile_error!("Either 'tungstenite' or 'fastwebsockets' feature must be enabled");

/// Unified WebSocket upgrade extractor that works with both backends
pub struct WebSocketUpgrade {
    #[cfg(feature = "tungstenite")]
    inner: axum::extract::ws::WebSocketUpgrade,
    #[cfg(all(feature = "fastwebsockets", not(feature = "tungstenite")))]
    inner: fastwebsockets::upgrade::IncomingUpgrade,
}

impl<S> FromRequestParts<S> for WebSocketUpgrade
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        #[cfg(feature = "tungstenite")]
        {
            let inner = axum::extract::ws::WebSocketUpgrade::from_request_parts(parts, state)
                .await
                .map_err(|e| e.into_response())?;
            Ok(Self { inner })
        }
        #[cfg(all(feature = "fastwebsockets", not(feature = "tungstenite")))]
        {
            use axum::http::StatusCode;
            let inner = fastwebsockets::upgrade::IncomingUpgrade::from_request_parts(parts, state)
                .await
                .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;
            Ok(Self { inner })
        }
    }
}

impl<S> OptionalFromRequestParts<S> for WebSocketUpgrade
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        #[cfg(feature = "tungstenite")]
        {
            match axum::extract::ws::WebSocketUpgrade::from_request_parts(parts, state).await {
                Ok(inner) => Ok(Some(Self { inner })),
                Err(_) => Ok(None), // Failed WebSocket upgrade becomes None
            }
        }
        #[cfg(all(feature = "fastwebsockets", not(feature = "tungstenite")))]
        {
            match fastwebsockets::upgrade::IncomingUpgrade::from_request_parts(parts, state).await {
                Ok(inner) => Ok(Some(Self { inner })),
                Err(_) => Ok(None), // Failed WebSocket upgrade becomes None
            }
        }
    }
}

impl WebSocketUpgrade {
    /// Perform the WebSocket upgrade with a handler
    pub async fn on_upgrade<S, I, O, C, F>(
        self,
        handler: Arc<WebSocketHandler<S, I, O, C, F>>,
        connection_id: String,
        cancellation_token: CancellationToken,
    ) -> Response
    where
        S: Send + Sync + 'static,
        I: Send + Sync + 'static,
        O: Send + Sync + 'static,
        C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
        F: StateFactory<S> + Send + Sync + Clone + 'static,
    {
        #[cfg(feature = "tungstenite")]
        {
            use crate::websocket_trait::AxumWebSocket;

            self.inner
                .on_upgrade(move |socket| async move {
                    let axum_ws = AxumWebSocket::new(socket);
                    if let Err(e) = handler
                        .start(axum_ws, connection_id, cancellation_token)
                        .await
                    {
                        tracing::error!("WebSocket handler error: {e:?}");
                    }
                })
                .into_response()
        }

        #[cfg(all(feature = "fastwebsockets", not(feature = "tungstenite")))]
        {
            use crate::websocket_trait::fast::FastWebSocket;

            let (response, fut) = self.inner.upgrade().unwrap();

            tokio::spawn(async move {
                match fut.await {
                    Ok(ws) => {
                        let fast_ws = FastWebSocket::new(ws);
                        if let Err(e) = handler
                            .start(fast_ws, connection_id, cancellation_token)
                            .await
                        {
                            tracing::error!("WebSocket handler error: {e:?}");
                        }
                    }
                    Err(e) => tracing::error!("WebSocket upgrade error: {e}"),
                }
            });

            response.into_response()
        }
    }
}

/// Extension trait for WebSocketHandler to provide a unified API
#[async_trait::async_trait]
pub trait UnifiedWebSocketExt<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    /// Handle WebSocket upgrade and return a response
    async fn handle_upgrade(
        self: Arc<Self>,
        ws: WebSocketUpgrade,
        connection_id: String,
        cancellation_token: CancellationToken,
    ) -> Response;
}

#[async_trait::async_trait]
impl<S, I, O, C, F> UnifiedWebSocketExt<S, I, O, C, F> for WebSocketHandler<S, I, O, C, F>
where
    S: Send + Sync + 'static,
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    C: MessageConverter<I, O> + Send + Sync + Clone + 'static,
    F: StateFactory<S> + Send + Sync + Clone + 'static,
{
    async fn handle_upgrade(
        self: Arc<Self>,
        ws: WebSocketUpgrade,
        connection_id: String,
        cancellation_token: CancellationToken,
    ) -> Response {
        ws.on_upgrade(self, connection_id, cancellation_token).await
    }
}
