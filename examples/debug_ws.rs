use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() {
    println!("Testing WebSocket echo behavior...\n");

    // Test tungstenite version
    println!("=== Testing Tungstenite (port 3000) ===");
    test_server("ws://127.0.0.1:3000/ws").await;

    println!("\n=== Testing FastWebSockets (port 3002) ===");
    test_server("ws://127.0.0.1:3002/ws").await;
}

async fn test_server(url: &str) {
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (mut write, mut read) = ws_stream.split();

    // Send test message
    let test_msg = "asdf";
    println!("Sending: {:?} (bytes: {:?})", test_msg, test_msg.as_bytes());
    write
        .send(Message::Text(test_msg.to_string().into()))
        .await
        .unwrap();

    // Read response
    if let Some(Ok(msg)) = read.next().await {
        match msg {
            Message::Text(text) => {
                println!("Received: {:?} (bytes: {:?})", text, text.as_bytes());
                println!("Length: {}", text.len());
                if text.contains('\n') {
                    println!("Contains newline at position: {:?}", text.find('\n'));
                }
            }
            _ => println!("Received non-text message"),
        }
    }

    write.send(Message::Close(None)).await.ok();
}
