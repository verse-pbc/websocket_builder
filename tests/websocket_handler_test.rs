//! Tests for the WebSocket handler

use anyhow::Result;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use websocket_builder::{
    ConnectionContext, DisconnectContext, InboundContext, MessageConverter, Middleware,
    OutboundContext, SendMessage, StateFactory, WebSocketBuilder,
};

// Simple test state
#[derive(Debug, Clone)]
struct TestState {
    counter: Arc<AtomicUsize>,
    messages: Arc<tokio::sync::Mutex<Vec<String>>>,
}

impl TestState {
    fn new() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            messages: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    async fn add_message(&self, msg: String) {
        self.messages.lock().await.push(msg);
    }

    fn increment(&self) -> usize {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }
}

// Simple message converter
#[derive(Clone)]
struct TestConverter;

impl MessageConverter<String, String> for TestConverter {
    fn inbound_from_string(&self, s: String) -> Result<Option<String>> {
        Ok(Some(s))
    }

    fn outbound_to_string(&self, s: String) -> Result<String> {
        Ok(s)
    }
}

// State factory
#[derive(Clone)]
struct TestStateFactory;

impl StateFactory<TestState> for TestStateFactory {
    fn create_state(&self, _token: CancellationToken) -> TestState {
        TestState::new()
    }
}

// Test middleware that increments counter
#[derive(Debug, Clone)]
struct CounterMiddleware;

#[async_trait]
impl Middleware for CounterMiddleware {
    type State = TestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            ctx.state
                .read()
                .await
                .add_message(format!("Inbound: {}", msg))
                .await;
            ctx.state.write().await.increment();
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            ctx.state
                .read()
                .await
                .add_message(format!("Outbound: {}", msg))
                .await;
        }
        ctx.next().await
    }

    async fn on_connect(
        &self,
        ctx: &mut ConnectionContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        ctx.state
            .read()
            .await
            .add_message("Connected".to_string())
            .await;
        if let Some(sender) = &mut ctx.sender {
            sender.send("Welcome".to_string())?;
        }
        Ok(())
    }

    async fn on_disconnect(
        &self,
        ctx: &mut DisconnectContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        ctx.state
            .read()
            .await
            .add_message("Disconnected".to_string())
            .await;
        Ok(())
    }
}

// Echo middleware that sends back messages
#[derive(Debug, Clone)]
struct EchoMiddleware;

#[async_trait]
impl Middleware for EchoMiddleware {
    type State = TestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &ctx.message {
            ctx.send_message(format!("Echo: {}", msg))?;
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        ctx.next().await
    }
}

#[tokio::test]
async fn test_actor_handler_creation() {
    let _handler = WebSocketBuilder::new(TestStateFactory, TestConverter)
        .with_middleware(CounterMiddleware)
        .build();

    // Test that handler was created successfully
    // The handler exists and can be used with a WebSocket
}

#[tokio::test]
async fn test_builder_with_multiple_middlewares() {
    let _handler = WebSocketBuilder::new(TestStateFactory, TestConverter)
        .with_middleware(CounterMiddleware)
        .with_middleware(EchoMiddleware)
        .with_channel_size(50)
        .build();

    // Test that we can build with multiple middlewares and custom channel size
}

#[tokio::test]
async fn test_middleware_trait_implementation() {
    // This test validates that middleware trait is implemented correctly
    // We can't directly test the middleware processing without proper context setup
    // which requires internal types, so we verify the middleware can be created
    let _counter = CounterMiddleware;
    let _echo = EchoMiddleware;

    // The actual middleware processing is tested through the builder integration
}

#[tokio::test]
async fn test_state_factory() {
    let factory = TestStateFactory;
    let token = CancellationToken::new();

    // Create multiple states
    let state1 = factory.create_state(token.clone());
    let state2 = factory.create_state(token.clone());

    // Each state should be independent
    state1.increment();
    state1.increment();
    state2.increment();

    assert_eq!(state1.counter.load(Ordering::SeqCst), 2);
    assert_eq!(state2.counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_message_converter() {
    let converter = TestConverter;

    // Test inbound conversion
    let inbound = converter.inbound_from_string("Hello".to_string()).unwrap();
    assert_eq!(inbound, Some("Hello".to_string()));

    // Test outbound conversion
    let outbound = converter.outbound_to_string("World".to_string()).unwrap();
    assert_eq!(outbound, "World");
}

// Middleware that adds prefixes to demonstrate order
#[derive(Debug, Clone)]
struct PrefixMiddleware(&'static str);

#[async_trait]
impl Middleware for PrefixMiddleware {
    type State = TestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &mut ctx.message {
            *msg = format!("{}:{}", self.0, msg);
            ctx.state
                .read()
                .await
                .add_message(format!("Processed by {}", self.0))
                .await;
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(msg) = &mut ctx.message {
            *msg = format!("{}:{}", self.0, msg);
        }
        ctx.next().await
    }
}

#[tokio::test]
async fn test_middleware_ordering() {
    // Create handler with multiple prefix middlewares
    let _handler = WebSocketBuilder::new(TestStateFactory, TestConverter)
        .with_middleware(PrefixMiddleware("First"))
        .with_middleware(PrefixMiddleware("Second"))
        .with_middleware(PrefixMiddleware("Third"))
        .build();

    // The middleware order should be preserved:
    // Inbound: First -> Second -> Third
    // Outbound: Third -> Second -> First (reverse)
}

#[test]
fn test_max_connections_configuration() {
    let handler = WebSocketBuilder::new(TestStateFactory, TestConverter)
        .with_max_connections(100)
        .build();

    // Handler should be created successfully with connection limit
    let _ = handler;
}

#[test]
fn test_max_connection_time_configuration() {
    use std::time::Duration;

    let handler = WebSocketBuilder::new(TestStateFactory, TestConverter)
        .with_max_connection_time(Duration::from_secs(3600)) // 1 hour
        .build();

    // Handler should be created successfully with connection timeout
    let _ = handler;
}
