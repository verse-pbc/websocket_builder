#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use axum::{routing::get, Router};
#[cfg(test)]
use futures_util::{SinkExt, StreamExt};
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::net::{TcpListener, TcpStream};
#[cfg(test)]
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
#[cfg(test)]
use tokio_util::sync::CancellationToken;
#[cfg(test)]
use websocket_builder::{
    MessageConverter, UnifiedWebSocketExt, WebSocketHandler, WebSocketUpgrade as WsUpgrade,
};

#[cfg(test)]
#[allow(dead_code)]
pub async fn create_websocket_client(
    proxy_addr: &str,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let url = format!("ws://{proxy_addr}");
    let (ws_stream, _) = connect_async(&url).await?;
    Ok(ws_stream)
}

#[cfg(test)]
#[allow(dead_code)]
pub async fn assert_proxy_response(
    client: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    message: &str,
    expected_response: &str,
) -> Result<()> {
    client
        .send(Message::Text(message.to_string().into()))
        .await?;

    if let Some(Ok(Message::Text(response))) = client.next().await {
        assert_eq!(response, expected_response);
        Ok(())
    } else {
        Err(anyhow::anyhow!("Expected text message"))
    }
}

#[cfg(test)]
#[derive(Clone)]
pub struct TestServer<T, I, O, Converter>
where
    T: Send + Sync + Clone + 'static + Default,
    I: Send + Sync + Clone + 'static,
    O: Send + Sync + Clone + 'static,
    Converter: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    ws_handler: WebSocketHandler<T, I, O, Converter>,
    shutdown: CancellationToken,
    state: T,
}

#[cfg(test)]
impl<T, I, O, Converter> TestServer<T, I, O, Converter>
where
    T: Send + Sync + Clone + 'static + Default,
    I: Send + Sync + Clone + 'static,
    O: Send + Sync + Clone + 'static,
    Converter: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    pub async fn start(
        addr: impl Into<String>,
        ws_handler: WebSocketHandler<T, I, O, Converter>,
        state: T,
    ) -> Result<Self> {
        let addr = addr.into();
        let listener = TcpListener::bind(&addr).await?;
        let shutdown = CancellationToken::new();
        let server = Self {
            ws_handler,
            shutdown: shutdown.clone(),
            state: state.clone(),
        };

        let server_state = Arc::new(server.clone());
        let server_state_clone = Arc::clone(&server_state);

        let app = Router::new()
            .route(
                "/",
                get(move |ws: WsUpgrade| {
                    let state = Arc::clone(&server_state_clone);
                    let addr = addr.clone();
                    let ws_handler = Arc::new(state.ws_handler.clone());
                    let conn_state = state.state.clone();
                    async move {
                        ws_handler
                            .handle_upgrade(ws, addr.clone(), state.shutdown.clone(), conn_state)
                            .await
                    }
                }),
            )
            .with_state(server_state);

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Ok(server)
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.shutdown.cancel();
        Ok(())
    }
}

#[cfg(test)]
#[allow(dead_code)]
/// Creates a test server with a dynamically assigned port.
/// Returns the server instance and the assigned address.
pub async fn create_test_server<T, I, O, Converter>(
    ws_handler: WebSocketHandler<T, I, O, Converter>,
    state: T,
) -> Result<(TestServer<T, I, O, Converter>, std::net::SocketAddr), anyhow::Error>
where
    T: Send + Sync + Clone + 'static + std::fmt::Debug + Default,
    I: Send + Sync + Clone + 'static,
    O: Send + Sync + Clone + 'static,
    Converter: MessageConverter<I, O> + Send + Sync + Clone + 'static,
{
    // Create a socket with port 0 to let the OS assign a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(listener); // Release the listener so our server can bind to this port

    println!("Using dynamically assigned port: {}", addr.port());

    let server = TestServer::start(addr.to_string(), ws_handler, state).await?;

    // Wait a bit for the server to be ready
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    Ok((server, addr))
}
