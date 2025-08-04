//! Flexible handler example
//!
//! This example shows how to handle both regular HTTP requests and WebSocket
//! upgrades on the SAME route using Option<WebSocketUpgrade>. When accessed
//! via a browser, it serves an HTML page. When accessed via WebSocket, it
//! handles the connection.

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo,
    },
    response::{Html, IntoResponse, Response},
    routing::any,
    Router,
};
use futures_util::{stream::SplitSink, SinkExt};
use std::net::SocketAddr;
use websocket_builder::{
    DisconnectReason, HandlerFactory, Utf8Bytes, WebSocketHandler, WebSocketUpgrade,
};

/// Simple echo handler
struct EchoHandler {
    addr: SocketAddr,
    sink: Option<SplitSink<WebSocket, Message>>,
}

impl WebSocketHandler for EchoHandler {
    async fn on_connect(
        &mut self,
        addr: SocketAddr,
        sink: SplitSink<WebSocket, Message>,
    ) -> Result<()> {
        self.addr = addr;
        self.sink = Some(sink);
        println!("Client connected from {addr}");

        // Send welcome message
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text(
                "Welcome! Send me a message and I'll echo it back.".into(),
            ))
            .await?;
        }
        Ok(())
    }

    async fn on_message(&mut self, text: Utf8Bytes) -> Result<()> {
        println!("Received from {}: {text}", self.addr);

        // Echo the message back
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text(format!("Echo: {text}").into()))
                .await?;
        }
        Ok(())
    }

    async fn on_disconnect(&mut self, reason: DisconnectReason) {
        println!("Client {} disconnected: {reason:?}", self.addr);
    }
}

/// Factory for creating echo handlers
#[derive(Clone)]
struct EchoHandlerFactory;

impl HandlerFactory for EchoHandlerFactory {
    type Handler = EchoHandler;

    fn create(&self, _headers: &axum::http::HeaderMap) -> Self::Handler {
        EchoHandler {
            addr: "0.0.0.0:0".parse().unwrap(),
            sink: None,
        }
    }
}

/// Handler that serves HTML or upgrades to WebSocket based on the request
/// This demonstrates how to conditionally handle WebSocket upgrades
async fn flexible_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    ws: Option<WebSocketUpgrade>,
) -> Response {
    match ws {
        Some(ws) => {
            // This is a WebSocket upgrade request
            println!("WebSocket upgrade request from {addr}");
            let handler = EchoHandlerFactory.create(&headers);
            websocket_builder::handle_upgrade(ws, addr, handler).await
        }
        None => {
            // This is a regular HTTP request - serve HTML
            println!("HTTP request from {addr}");
            Html(HTML_PAGE).into_response()
        }
    }
}

const HTML_PAGE: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <title>WebSocket Flexible Handler Demo</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            max-width: 800px;
            margin: 50px auto;
            padding: 20px;
        }
        .highlight {
            background-color: #f0f0f0;
            padding: 10px;
            border-radius: 5px;
            margin: 20px 0;
        }
        #messages {
            border: 1px solid #ccc;
            height: 300px;
            overflow-y: auto;
            padding: 10px;
            margin-bottom: 10px;
            background-color: #f5f5f5;
        }
        .message {
            margin: 5px 0;
            padding: 5px;
            border-radius: 3px;
        }
        .sent {
            background-color: #e3f2fd;
            text-align: right;
        }
        .received {
            background-color: #f5f5f5;
            text-align: left;
        }
        .system {
            background-color: #fff3cd;
            text-align: center;
            font-style: italic;
        }
        input[type="text"] {
            width: 70%;
            padding: 10px;
        }
        button {
            padding: 10px 20px;
            margin-left: 10px;
        }
        code {
            background-color: #f0f0f0;
            padding: 2px 5px;
            border-radius: 3px;
        }
    </style>
</head>
<body>
    <h1>Flexible Handler Demo</h1>
    
    <div class="highlight">
        <p><strong>This page demonstrates how the SAME route can handle both HTTP and WebSocket!</strong></p>
        <p>The current URL served this HTML page via HTTP GET.</p>
        <p>But if you connect via WebSocket to the SAME URL, it will handle the WebSocket connection.</p>
    </div>

    <h2>Try it in the browser console:</h2>
    <pre><code>// Connect to the SAME URL with WebSocket
const ws = new WebSocket('ws://' + window.location.host + window.location.pathname);
ws.onmessage = (e) => console.log('Received:', e.data);
ws.onopen = () => {
    console.log('Connected!');
    ws.send('Hello from console!');
};</code></pre>

    <h2>Or use the interactive client below:</h2>
    <div id="messages"></div>
    <div>
        <input type="text" id="messageInput" placeholder="Type a message..." />
        <button id="sendButton">Send</button>
        <button id="connectButton">Connect to Same URL</button>
    </div>

    <script>
        let ws = null;
        const messages = document.getElementById('messages');
        const messageInput = document.getElementById('messageInput');
        const sendButton = document.getElementById('sendButton');
        const connectButton = document.getElementById('connectButton');

        function addMessage(text, className) {
            const div = document.createElement('div');
            div.className = 'message ' + className;
            div.textContent = text;
            messages.appendChild(div);
            messages.scrollTop = messages.scrollHeight;
        }

        function connect() {
            // Connect to the SAME URL that served this page!
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}${window.location.pathname}`;
            
            addMessage(`Connecting to ${wsUrl} (same URL as this page!)`, 'system');
            ws = new WebSocket(wsUrl);
            
            ws.onopen = () => {
                messageInput.disabled = false;
                sendButton.disabled = false;
                connectButton.textContent = 'Disconnect';
                addMessage('Connected! Now using WebSocket on the same route.', 'system');
            };
            
            ws.onmessage = (event) => {
                addMessage(event.data, 'received');
            };
            
            ws.onclose = () => {
                messageInput.disabled = true;
                sendButton.disabled = true;
                connectButton.textContent = 'Connect to Same URL';
                addMessage('Disconnected from WebSocket', 'system');
                ws = null;
            };
            
            ws.onerror = (error) => {
                addMessage('Error: ' + error, 'system');
            };
        }

        function disconnect() {
            if (ws) {
                ws.close();
            }
        }

        function sendMessage() {
            const message = messageInput.value.trim();
            if (message && ws && ws.readyState === WebSocket.OPEN) {
                ws.send(message);
                addMessage(message, 'sent');
                messageInput.value = '';
            }
        }

        connectButton.onclick = () => {
            if (ws) {
                disconnect();
            } else {
                connect();
            }
        };

        sendButton.onclick = sendMessage;
        sendButton.disabled = true;
        messageInput.disabled = true;

        messageInput.onkeypress = (event) => {
            if (event.key === 'Enter') {
                sendMessage();
            }
        };
    </script>
</body>
</html>
"#;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build our application with a SINGLE route that handles both HTTP and WebSocket
    let app = Router::new().route("/", any(flexible_handler));

    // Run it
    let addr = "127.0.0.1:3000";
    println!("Listening on http://{addr}");
    println!("Open http://127.0.0.1:3000 in your browser");
    println!("The SAME route handles both HTTP (serves HTML) and WebSocket connections!");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
