# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

WebSocket Builder is a simple, trait-based WebSocket framework for Rust with Axum integration. It provides a minimal abstraction over WebSocket connections with per-connection handler instances for natural state management.

## Core Architecture

The library follows a three-layer architecture:

1. **Handler Layer** (`src/handler.rs`): Defines the `WebSocketHandler` trait with three essential methods:
   - `on_connect`: Called when connection established, receives the sink for sending messages
   - `on_message`: Processes incoming text messages (binary messages are ignored by design)
   - `on_disconnect`: Cleanup when connection closes

2. **Connection Layer** (`src/connection.rs`): Manages WebSocket lifecycle:
   - Splits socket into sink/stream
   - Routes messages to handler methods
   - Handles protocol details (ping/pong, close frames)
   - Manages graceful disconnection

3. **Axum Integration** (`src/axum.rs`): Provides web framework integration:
   - `websocket_route`: Creates router with WebSocket endpoint
   - `WebSocketUpgrade`: Optional extractor for flexible HTTP/WebSocket routes
   - Connection limiting via semaphores
   - Per-connection handler instantiation via `HandlerFactory`

## Common Development Commands

### Building and Testing
```bash
# Build the library
cargo build

# Run all tests (unit + integration)
cargo test

# Run tests with output visible
cargo test -- --nocapture

# Build and test all examples
cargo build --examples
cargo test --examples

# Run specific example
cargo run --example echo_server
cargo run --example chat_server
cargo run --example flexible_handler
```

### Code Quality
```bash
# Format code
cargo fmt

# Check formatting without changes
cargo fmt -- --check

# Run clippy with strict settings (matching CI)
cargo clippy --all-targets --all-features -- -D warnings

# Generate documentation
cargo doc --open
```

### CI/CD Checks (run before committing)
```bash
# Run the same checks as CI
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-features
cargo test --all-features

# Build all examples
cargo build --example echo_server
cargo build --example chat_server
cargo build --example flexible_handler
```

### Git Hooks Setup
```bash
# Install pre-commit hooks for automatic formatting and linting
./scripts/setup-hooks.sh
```

## Key Design Decisions

1. **Per-Connection Handler Instances**: Each WebSocket connection gets its own handler instance, eliminating shared state complexity and race conditions.

2. **Text-Only Messages**: Binary messages are intentionally ignored to keep the API simple. Use underlying Axum functionality if binary support is needed.

3. **Sink Ownership**: Handlers own their sink after `on_connect`, enabling flexible message sending patterns without Arc/Mutex overhead.

4. **Trait-Based API**: Simple trait with only 3 required methods makes implementation straightforward while remaining flexible.

5. **Factory Pattern**: `HandlerFactory` creates handlers with access to HTTP headers, enabling context extraction (auth tokens, subdomains, etc.).

## Testing Strategy

### Unit Tests
- Embedded in source files within `#[cfg(test)]` modules
- Test individual components in isolation
- Located in: `src/handler.rs`, `src/connection.rs`, `src/axum.rs`

### Integration Tests
- Full WebSocket client-server tests in `tests/integration_test.rs`
- Uses `tokio-tungstenite` for client implementation
- Tests complete connection lifecycle

### Example Testing
```bash
# Test connection limiting
cargo test test_semaphore_connection_limiting

# Test with specific example
timeout 2 cargo run --example echo_server  # In one terminal
# Then connect with WebSocket client in another terminal
```

## Performance Considerations

1. **Connection Limits**: Use `ConnectionConfig::max_connections` to prevent resource exhaustion
2. **Message Handling**: Avoid blocking operations in handler methods
3. **Sink Management**: Store sink in handler for efficient message sending
4. **Error Handling**: Return errors from handlers to trigger graceful disconnection

## Common Patterns

### Basic Handler Implementation
```rust
struct MyHandler {
    sink: Option<SplitSink<WebSocket, Message>>,
}

impl WebSocketHandler for MyHandler {
    async fn on_connect(&mut self, addr: SocketAddr, sink: SplitSink<WebSocket, Message>) -> Result<()> {
        self.sink = Some(sink);
        // Send welcome message
        if let Some(sink) = &mut self.sink {
            sink.send(Message::Text("Welcome!".into())).await?;
        }
        Ok(())
    }
    // ... other methods
}
```

### Flexible HTTP/WebSocket Route
```rust
async fn handler(ws: Option<WebSocketUpgrade>) -> Response {
    match ws {
        Some(ws) => handle_upgrade(ws, addr, handler).await,
        None => Html("<h1>HTTP Response</h1>").into_response(),
    }
}
```

## Debugging WebSocket Issues

1. **Enable logging**: Set `RUST_LOG=debug` for detailed connection logs
2. **Check connection limits**: Verify semaphore permits if connections are rejected
3. **Test with examples**: Use provided examples as reference implementations
4. **Browser console**: Check for WebSocket errors in browser developer tools

## Library Limitations

This library prioritizes simplicity. For these features, use Axum WebSocket directly:
- Binary message handling
- Custom protocol extensions
- Complex middleware chains
- Multiple message types per handler