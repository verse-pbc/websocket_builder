# WebSocket Builder Examples

This directory contains examples demonstrating various features of the websocket_builder library.

## Examples

### basic_demo.rs
Basic example showing how to create a WebSocket server with simple middleware.

```bash
cargo run --example basic_demo
```

### pipeline_demo.rs
Comprehensive middleware pipeline example demonstrating:
- Connection tracking middleware
- Logging middleware  
- Message transformation middleware
- Stats middleware

```bash
cargo run --example pipeline_demo
```

### debug_ws.rs
Simple WebSocket debugging tool.

```bash
cargo run --example debug_ws
```

## Backend Selection

The library supports both tungstenite (default) and fastwebsockets backends. Switch between them using feature flags in your Cargo.toml:

```toml
# Use tungstenite (default)
websocket_builder = "0.2.0-alpha.1"

# Use fastwebsockets only
websocket_builder = { version = "0.1.0-alpha.1", default-features = false, features = ["fastwebsockets"] }
```

## Testing WebSocket Connections

All examples can be tested with `websocat`:

```bash
# Install websocat
cargo install websocat

# Test connection
websocat ws://127.0.0.1:3001/ws

# Send messages
echo "Hello" | websocat ws://127.0.0.1:3001/ws -n
```

## Choosing a Backend

- **Tungstenite** (default): Maximum compatibility, well-tested, mature
- **FastWebSockets**: High performance, low latency, minimal overhead