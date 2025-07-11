//! Tests for the Middleware trait and its default implementations

use anyhow::Result;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use websocket_builder::{
    ConnectionContext, DisconnectContext, InboundContext, Middleware, OutboundContext,
};

// Test state for middleware testing
#[derive(Debug, Clone)]
struct MiddlewareTestState {
    counter: Arc<AtomicUsize>,
    messages: Arc<parking_lot::Mutex<Vec<String>>>,
    connect_calls: Arc<AtomicUsize>,
    disconnect_calls: Arc<AtomicUsize>,
    inbound_calls: Arc<AtomicUsize>,
    outbound_calls: Arc<AtomicUsize>,
}

impl Default for MiddlewareTestState {
    fn default() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            connect_calls: Arc::new(AtomicUsize::new(0)),
            disconnect_calls: Arc::new(AtomicUsize::new(0)),
            inbound_calls: Arc::new(AtomicUsize::new(0)),
            outbound_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl MiddlewareTestState {
    fn add_message(&self, msg: String) {
        self.messages.lock().push(msg);
    }

    fn get_messages(&self) -> Vec<String> {
        self.messages.lock().clone()
    }

    fn get_counter(&self) -> usize {
        self.counter.load(Ordering::SeqCst)
    }

    fn increment_connect_calls(&self) -> usize {
        self.connect_calls.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn increment_disconnect_calls(&self) -> usize {
        self.disconnect_calls.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn increment_inbound_calls(&self) -> usize {
        self.inbound_calls.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn increment_outbound_calls(&self) -> usize {
        self.outbound_calls.fetch_add(1, Ordering::SeqCst) + 1
    }
}

// Middleware that uses all default implementations to test them
#[derive(Debug, Clone)]
struct DefaultMiddleware {
    _name: String,
}

impl DefaultMiddleware {
    fn new(name: &str) -> Self {
        Self {
            _name: name.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for DefaultMiddleware {
    type State = MiddlewareTestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    // All methods use default implementations
    // This tests the default trait implementations
}

// Middleware that overrides all methods to test custom implementations
#[derive(Debug, Clone)]
struct CustomMiddleware {
    _name: String,
}

impl CustomMiddleware {
    fn new(name: &str) -> Self {
        Self {
            _name: name.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for CustomMiddleware {
    type State = MiddlewareTestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        let count = ctx.state.write().increment_inbound_calls();
        let msg = format!("{}: inbound #{}", self._name, count);
        ctx.state.read().add_message(msg);

        if let Some(message) = &ctx.message {
            ctx.message = Some(format!("{}({})", self._name, message));
        }

        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        let count = ctx.state.write().increment_outbound_calls();
        let msg = format!("{}: outbound #{}", self._name, count);
        ctx.state.read().add_message(msg);

        if let Some(message) = &ctx.message {
            ctx.message = Some(format!("Out{}({})", self._name, message));
        }

        ctx.next().await
    }

    async fn on_connect(
        &self,
        ctx: &mut ConnectionContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        let count = ctx.state.write().increment_connect_calls();
        let msg = format!("{}: connect #{}", self._name, count);
        ctx.state.read().add_message(msg);

        // Send welcome message
        if let Some(sender) = &mut ctx.sender {
            sender.send(format!("Welcome from {}", self._name))?;
        }

        ctx.next().await
    }

    async fn on_disconnect(
        &self,
        ctx: &mut DisconnectContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        let count = ctx.state.write().increment_disconnect_calls();
        let msg = format!("{}: disconnect #{}", self._name, count);
        ctx.state.read().add_message(msg);
        ctx.next().await
    }
}

// Error-generating middleware to test error paths
#[derive(Debug, Clone)]
struct ErrorMiddleware {
    fail_on: String,
    name: String,
}

impl ErrorMiddleware {
    fn new(name: &str, fail_on: &str) -> Self {
        Self {
            name: name.to_string(),
            fail_on: fail_on.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for ErrorMiddleware {
    type State = MiddlewareTestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if self.fail_on == "inbound" {
            return Err(anyhow::anyhow!("{}: Simulated inbound error", self.name));
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if self.fail_on == "outbound" {
            return Err(anyhow::anyhow!("{}: Simulated outbound error", self.name));
        }
        ctx.next().await
    }

    async fn on_connect(
        &self,
        ctx: &mut ConnectionContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if self.fail_on == "connect" {
            return Err(anyhow::anyhow!("{}: Simulated connect error", self.name));
        }
        ctx.next().await
    }

    async fn on_disconnect(
        &self,
        ctx: &mut DisconnectContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if self.fail_on == "disconnect" {
            return Err(anyhow::anyhow!("{}: Simulated disconnect error", self.name));
        }
        ctx.next().await
    }
}

// Message transformation middleware
#[derive(Debug, Clone)]
struct TransformMiddleware {
    prefix: String,
    suffix: String,
}

impl TransformMiddleware {
    fn new(prefix: &str, suffix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for TransformMiddleware {
    type State = MiddlewareTestState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(message) = &ctx.message {
            ctx.message = Some(format!("{}{}{}", self.prefix, message, self.suffix));
        }
        ctx.next().await
    }

    async fn process_outbound(
        &self,
        ctx: &mut OutboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<()> {
        if let Some(message) = &ctx.message {
            ctx.message = Some(format!("OUT{}{}{}", self.prefix, message, self.suffix));
        }
        ctx.next().await
    }
}

// Helper to create mock contexts
type MockSenderReceiver = (
    flume::Sender<(String, usize)>,
    flume::Receiver<(String, usize)>,
);

fn create_mock_sender() -> MockSenderReceiver {
    flume::bounded(10)
}

fn create_middlewares() -> Vec<
    Arc<
        dyn Middleware<
            State = MiddlewareTestState,
            IncomingMessage = String,
            OutgoingMessage = String,
        >,
    >,
> {
    // Return empty vec for testing contexts independently
    vec![]
}

fn create_single_middleware() -> Vec<
    Arc<
        dyn Middleware<
            State = MiddlewareTestState,
            IncomingMessage = String,
            OutgoingMessage = String,
        >,
    >,
> {
    vec![Arc::new(DefaultMiddleware::new("dummy"))]
}

#[tokio::test]
async fn test_default_middleware_implementations() {
    let middleware = DefaultMiddleware::new("default");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();
    let (sender, _rx) = create_mock_sender();

    // Test default process_inbound
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = InboundContext::new(
        "test_conn".to_string(),
        Some("test_message".to_string()),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.process_inbound(&mut ctx).await;
    assert!(result.is_ok(), "Default process_inbound should succeed");

    // Test default process_outbound
    let mut ctx = OutboundContext::new(
        "test_conn".to_string(),
        "test_outbound".to_string(),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.process_outbound(&mut ctx).await;
    assert!(result.is_ok(), "Default process_outbound should succeed");

    // Test default on_connect
    let mut ctx = ConnectionContext::new(
        "test_conn".to_string(),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.on_connect(&mut ctx).await;
    assert!(result.is_ok(), "Default on_connect should succeed");

    // Test default on_disconnect
    let mut ctx = DisconnectContext::new(
        "test_conn".to_string(),
        Some(sender),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.on_disconnect(&mut ctx).await;
    assert!(result.is_ok(), "Default on_disconnect should succeed");
}

#[tokio::test]
async fn test_custom_middleware_implementations() {
    let middleware = CustomMiddleware::new("custom");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();
    let (sender, _rx) = create_mock_sender();

    // Test custom process_inbound
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = InboundContext::new(
        "test_conn".to_string(),
        Some("hello".to_string()),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.process_inbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, Some("custom(hello)".to_string()));
    assert_eq!(ctx.state.read().inbound_calls.load(Ordering::SeqCst), 1);

    // Test custom process_outbound
    let mut ctx = OutboundContext::new(
        "test_conn".to_string(),
        "goodbye".to_string(),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.process_outbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, Some("Outcustom(goodbye)".to_string()));
    assert_eq!(ctx.state.read().outbound_calls.load(Ordering::SeqCst), 1);

    // Test custom on_connect
    let mut ctx = ConnectionContext::new(
        "test_conn".to_string(),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.on_connect(&mut ctx).await;
    if let Err(e) = &result {
        println!("on_connect error: {e}");
    }
    assert!(result.is_ok());
    assert_eq!(ctx.state.read().connect_calls.load(Ordering::SeqCst), 1);

    // Test custom on_disconnect
    let mut ctx = DisconnectContext::new(
        "test_conn".to_string(),
        Some(sender),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.on_disconnect(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.state.read().disconnect_calls.load(Ordering::SeqCst), 1);

    // Verify messages were logged
    let messages = ctx.state.read().get_messages();
    assert!(messages.contains(&"custom: inbound #1".to_string()));
    assert!(messages.contains(&"custom: outbound #1".to_string()));
    assert!(messages.contains(&"custom: connect #1".to_string()));
    assert!(messages.contains(&"custom: disconnect #1".to_string()));
}

#[tokio::test]
async fn test_error_middleware_inbound_error() {
    let middleware = ErrorMiddleware::new("error_test", "inbound");
    let state = MiddlewareTestState::default();
    let middlewares = create_middlewares();
    let (sender, _rx) = create_mock_sender();

    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = InboundContext::new(
        "test_conn".to_string(),
        Some("test".to_string()),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.process_inbound(&mut ctx).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("error_test"));
    assert!(error_msg.contains("Simulated inbound error"));
}

#[tokio::test]
async fn test_error_middleware_outbound_error() {
    let middleware = ErrorMiddleware::new("error_test", "outbound");
    let state = MiddlewareTestState::default();
    let middlewares = create_middlewares();
    let (sender, _rx) = create_mock_sender();

    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = OutboundContext::new(
        "test_conn".to_string(),
        "test".to_string(),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.process_outbound(&mut ctx).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("error_test"));
    assert!(error_msg.contains("Simulated outbound error"));
}

#[tokio::test]
async fn test_error_middleware_connect_error() {
    let middleware = ErrorMiddleware::new("error_test", "connect");
    let state = MiddlewareTestState::default();
    let middlewares = create_middlewares();
    let (sender, _rx) = create_mock_sender();

    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = ConnectionContext::new(
        "test_conn".to_string(),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.on_connect(&mut ctx).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("error_test"));
    assert!(error_msg.contains("Simulated connect error"));
}

#[tokio::test]
async fn test_error_middleware_disconnect_error() {
    let middleware = ErrorMiddleware::new("error_test", "disconnect");
    let state = MiddlewareTestState::default();
    let middlewares = create_middlewares();
    let (sender, _rx) = create_mock_sender();

    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = DisconnectContext::new(
        "test_conn".to_string(),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.on_disconnect(&mut ctx).await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("error_test"));
    assert!(error_msg.contains("Simulated disconnect error"));
}

#[tokio::test]
async fn test_transform_middleware_message_transformation() {
    let middleware = TransformMiddleware::new("[", "]");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();
    let (sender, _rx) = create_mock_sender();

    // Test inbound transformation
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = InboundContext::new(
        "test_conn".to_string(),
        Some("hello".to_string()),
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.process_inbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, Some("[hello]".to_string()));

    // Test outbound transformation
    let mut ctx = OutboundContext::new(
        "test_conn".to_string(),
        "world".to_string(),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.process_outbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, Some("OUT[world]".to_string()));
}

#[tokio::test]
async fn test_middleware_with_none_message() {
    let middleware = CustomMiddleware::new("none_test");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();
    let (sender, _rx) = create_mock_sender();

    // Test inbound with None message
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = InboundContext::new(
        "test_conn".to_string(),
        None,
        Some(sender.clone()),
        state_arc.clone(),
        middlewares_arc.clone(),
        0,
    );

    let result = middleware.process_inbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, None); // Should remain None

    // Test outbound processing with message
    let mut ctx = OutboundContext::new(
        "test_conn".to_string(),
        "test".to_string(),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.process_outbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, Some("Outnone_test(test)".to_string()));
}

#[tokio::test]
async fn test_middleware_without_sender() {
    let middleware = CustomMiddleware::new("no_sender");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();

    // Test on_connect without sender
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = ConnectionContext::new(
        "test_conn".to_string(),
        None, // No sender
        state_arc.clone(),
        middlewares_arc,
        0,
    );

    let result = middleware.on_connect(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.state.read().connect_calls.load(Ordering::SeqCst), 1);

    // Verify message was still logged even without sender
    let messages = ctx.state.read().get_messages();
    assert!(messages.contains(&"no_sender: connect #1".to_string()));
}

#[tokio::test]
async fn test_middleware_trait_bounds() {
    // Test that our middleware implements the required traits
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    fn assert_debug<T: std::fmt::Debug>() {}

    assert_send::<DefaultMiddleware>();
    assert_sync::<DefaultMiddleware>();
    assert_debug::<DefaultMiddleware>();

    assert_send::<CustomMiddleware>();
    assert_sync::<CustomMiddleware>();
    assert_debug::<CustomMiddleware>();

    assert_send::<ErrorMiddleware>();
    assert_sync::<ErrorMiddleware>();
    assert_debug::<ErrorMiddleware>();

    assert_send::<TransformMiddleware>();
    assert_sync::<TransformMiddleware>();
    assert_debug::<TransformMiddleware>();
}

#[tokio::test]
async fn test_middleware_state_mutations() {
    let middleware = CustomMiddleware::new("state_test");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();
    let (sender, _rx) = create_mock_sender();

    // Initially state should be default
    assert_eq!(state.get_counter(), 0);

    // Process multiple messages to verify state mutations
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    for i in 1..=5 {
        let mut ctx = InboundContext::new(
            "test_conn".to_string(),
            Some(format!("msg{i}")),
            Some(sender.clone()),
            state_arc.clone(),
            middlewares_arc.clone(),
            0,
        );

        let result = middleware.process_inbound(&mut ctx).await;
        assert!(result.is_ok());
    }

    // Verify state was mutated
    assert_eq!(state_arc.read().inbound_calls.load(Ordering::SeqCst), 5);

    let messages = state_arc.read().get_messages();
    assert_eq!(messages.len(), 5);
    assert!(messages.contains(&"state_test: inbound #1".to_string()));
    assert!(messages.contains(&"state_test: inbound #5".to_string()));
}

#[tokio::test]
async fn test_middleware_debug_implementations() {
    let default_mw = DefaultMiddleware::new("debug_test");
    let custom_mw = CustomMiddleware::new("debug_test");
    let error_mw = ErrorMiddleware::new("debug_test", "none");
    let transform_mw = TransformMiddleware::new("pre", "post");

    // Test that Debug is implemented (should not panic)
    let debug_str = format!("{default_mw:?}");
    assert!(debug_str.contains("DefaultMiddleware"));

    let debug_str = format!("{custom_mw:?}");
    assert!(debug_str.contains("CustomMiddleware"));

    let debug_str = format!("{error_mw:?}");
    assert!(debug_str.contains("ErrorMiddleware"));

    let debug_str = format!("{transform_mw:?}");
    assert!(debug_str.contains("TransformMiddleware"));
}

#[tokio::test]
async fn test_middleware_clone_implementations() {
    let default_mw = DefaultMiddleware::new("clone_test");
    let custom_mw = CustomMiddleware::new("clone_test");
    let error_mw = ErrorMiddleware::new("clone_test", "none");
    let transform_mw = TransformMiddleware::new("pre", "post");

    // Test that Clone is implemented (should not panic)
    let _cloned_default = default_mw.clone();
    let _cloned_custom = custom_mw.clone();
    let _cloned_error = error_mw.clone();
    let _cloned_transform = transform_mw.clone();
}

#[tokio::test]
async fn test_middleware_with_empty_string_messages() {
    let middleware = TransformMiddleware::new("", "");
    let state = MiddlewareTestState::default();
    let middlewares = create_single_middleware();
    let (sender, _rx) = create_mock_sender();

    // Test with empty string message
    let state_arc = Arc::new(parking_lot::RwLock::new(state));
    let middlewares_arc = Arc::new(middlewares);
    let mut ctx = InboundContext::new(
        "test_conn".to_string(),
        Some("".to_string()),
        Some(sender),
        state_arc,
        middlewares_arc,
        0,
    );

    let result = middleware.process_inbound(&mut ctx).await;
    assert!(result.is_ok());
    assert_eq!(ctx.message, Some("".to_string())); // Empty prefix + empty message + empty suffix = empty string
}
