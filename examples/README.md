# WebSocket Builder Examples

This directory contains examples demonstrating how to use the websocket_builder library.

## Examples

### echo_server.rs
A simple WebSocket echo server that sends back any message it receives.

```bash
cargo run --example echo_server
```

Then connect with a WebSocket client to `ws://localhost:3000/ws`.

### chat_server.rs
A multi-user chat room with usernames and broadcast messaging.

```bash
cargo run --example chat_server
```

Connect multiple WebSocket clients to `ws://localhost:3000/ws` and:
1. Set your username with `/name <username>`
2. Send messages that will be broadcast to all connected users

### flexible_handler.rs
Shows how the same route can handle both regular HTTP and WebSocket requests.

```bash
cargo run --example flexible_handler
```

Open `http://localhost:3000` in your browser to see an HTML page with a built-in WebSocket client. The same route serves the HTML page on regular GET requests and handles WebSocket upgrades.

## Testing with a WebSocket Client

You can test these examples using various WebSocket clients:

### Using websocat
```bash
# Install websocat
cargo install websocat

# Connect to echo server
websocat ws://localhost:3000/ws

# Connect to chat server
websocat ws://localhost:3000/ws
> /name Alice
> Hello everyone!
```

### Using wscat (Node.js)
```bash
# Install wscat
npm install -g wscat

# Connect
wscat -c ws://localhost:3000/ws
```

### Using a browser
Open the browser console and run:
```javascript
const ws = new WebSocket('ws://localhost:3000/ws');
ws.onmessage = (event) => console.log('Received:', event.data);
ws.onopen = () => {
    ws.send('Hello from browser!');
};
```