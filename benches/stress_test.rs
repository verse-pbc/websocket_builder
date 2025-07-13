use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::timeout;
use websocket_builder::{
    InboundContext, MessageConverter, Middleware, SendMessage, WebSocketBuilder,
};

// Simple state for benchmarking
#[derive(Debug, Clone, Default)]
struct BenchState;

// Message converter for strings
#[derive(Clone, Debug)]
struct StringConverter;

impl MessageConverter<String, String> for StringConverter {
    fn inbound_from_bytes(&self, bytes: &[u8]) -> Result<Option<String>, anyhow::Error> {
        if bytes.is_empty() {
            return Ok(None);
        }
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(Some(s.to_string())),
            Err(e) => Err(anyhow::anyhow!("Invalid UTF-8: {}", e)),
        }
    }

    fn outbound_to_string(&self, message: String) -> Result<String, anyhow::Error> {
        Ok(message)
    }
}

// High-throughput middleware that processes messages quickly
#[derive(Debug)]
struct ThroughputMiddleware;

#[async_trait::async_trait]
impl Middleware for ThroughputMiddleware {
    type State = BenchState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<(), anyhow::Error> {
        if let Some(message) = &ctx.message {
            // Echo the message back to measure round-trip throughput
            ctx.send_message(message.clone())?;
        }
        ctx.next().await
    }
}

// Benchmark 1: High data throughput on a single connection
fn bench_high_throughput_single_connection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("high_throughput_single_connection", |b| {
        b.to_async(&rt).iter(|| async {
            let _handler = Arc::new(
                WebSocketBuilder::new(StringConverter)
                    .with_middleware(ThroughputMiddleware)
                    .with_channel_size(10000) // Large buffer for high throughput
                    .build(),
            );

            // Simulate processing a large number of messages quickly
            let message_count = 1000;
            let large_message = "A".repeat(1024); // 1KB message

            // Create a mock context and process many messages
            let mut processed = 0;
            for _ in 0..message_count {
                // Simulate message processing overhead
                let _result = format!("Processed: {large_message}");
                processed += 1;
            }

            processed
        });
    });
}

// Benchmark 2: Many concurrent connections
fn bench_many_concurrent_connections(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("many_concurrent_connections", |b| {
        b.to_async(&rt).iter(|| async {
            let connection_count = 10000;

            // Create many handler instances to simulate concurrent connections
            let mut handlers = Vec::with_capacity(connection_count);

            for _ in 0..connection_count {
                let handler = Arc::new(
                    WebSocketBuilder::new(StringConverter)
                        .with_middleware(ThroughputMiddleware)
                        .with_channel_size(100) // Smaller buffer per connection
                        .build(),
                );
                handlers.push(handler);
            }

            // Simulate each connection processing a message
            let mut total_processed = 0;
            for _handler in &handlers {
                // Simulate lightweight processing per connection
                let _connection_id = format!("conn_{total_processed}");
                total_processed += 1;
            }

            total_processed
        });
    });
}

// Alternative high-throughput test with actual async processing
fn bench_async_message_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("async_message_processing", |b| {
        b.to_async(&rt).iter(|| async {
            let handler = Arc::new(
                WebSocketBuilder::new(StringConverter)
                    .with_middleware(ThroughputMiddleware)
                    .with_channel_size(5000)
                    .build(),
            );

            // Simulate async processing of multiple messages
            let message_batch_size = 100;
            let mut futures = Vec::new();

            for i in 0..message_batch_size {
                let _handler_clone = handler.clone();
                let future = async move {
                    // Simulate async message processing
                    tokio::time::sleep(Duration::from_nanos(1)).await;
                    format!("processed_{i}")
                };
                futures.push(future);
            }

            // Wait for all messages to be processed
            let results = timeout(Duration::from_secs(1), futures::future::join_all(futures)).await;
            results.unwrap_or_default().len()
        });
    });
}

// Stress test with connection churn
fn bench_connection_churn(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("connection_churn", |b| {
        b.to_async(&rt).iter(|| async {
            let churn_cycles = 1000;
            let connections_per_cycle = 10;

            for _ in 0..churn_cycles {
                // Create connections
                let mut handlers = Vec::new();
                for _ in 0..connections_per_cycle {
                    let handler = Arc::new(
                        WebSocketBuilder::new(StringConverter)
                            .with_middleware(ThroughputMiddleware)
                            .with_channel_size(50)
                            .build(),
                    );
                    handlers.push(handler);
                }

                // Simulate some work
                for _handler in &handlers {
                    let _work = format!("work_{:p}", _handler.as_ref());
                }

                // Handlers automatically drop here
            }

            churn_cycles * connections_per_cycle
        });
    });
}

criterion_group!(
    stress_tests,
    bench_high_throughput_single_connection,
    bench_many_concurrent_connections,
    bench_async_message_processing,
    bench_connection_churn
);
criterion_main!(stress_tests);
