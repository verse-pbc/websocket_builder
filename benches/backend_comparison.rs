use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use tokio::runtime::Runtime;
use websocket_builder::{
    InboundContext, MessageConverter, Middleware, SendMessage, WebSocketBuilder,
};

// Simple state
#[derive(Debug, Clone, Default)]
struct BenchState;

// Message converter
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

    fn outbound_to_bytes(
        &self,
        message: String,
    ) -> Result<std::borrow::Cow<'_, [u8]>, anyhow::Error> {
        Ok(std::borrow::Cow::Owned(message.into_bytes()))
    }
}

// Echo middleware
#[derive(Debug)]
struct EchoMiddleware;

#[async_trait::async_trait]
impl Middleware for EchoMiddleware {
    type State = BenchState;
    type IncomingMessage = String;
    type OutgoingMessage = String;

    async fn process_inbound(
        &self,
        ctx: &mut InboundContext<Self::State, Self::IncomingMessage, Self::OutgoingMessage>,
    ) -> Result<(), anyhow::Error> {
        if let Some(message) = &ctx.message {
            ctx.send_message(message.clone())?;
        }
        ctx.next().await
    }
}

fn bench_handler_creation(c: &mut Criterion) {
    c.bench_function("create_handler", |b| {
        b.iter(|| {
            WebSocketBuilder::new(StringConverter)
                .with_middleware(EchoMiddleware)
                .with_channel_size(100)
                .build()
        });
    });
}

fn bench_middleware_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("middleware_chain");

    for count in [1, 3, 5].iter() {
        group.bench_with_input(format!("{count}_middlewares"), count, |b, &count| {
            b.iter(|| {
                let mut builder = WebSocketBuilder::new(StringConverter);
                for _ in 0..count {
                    builder = builder.with_middleware(EchoMiddleware);
                }
                builder.build()
            });
        });
    }

    group.finish();
}

fn bench_concurrent_handlers(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_handlers");

    for count in [10, 50, 100].iter() {
        group.bench_with_input(format!("{count}_handlers"), count, |b, &count| {
            b.to_async(&rt).iter(|| async move {
                let handlers: Vec<_> = (0..count)
                    .map(|_| {
                        Arc::new(
                            WebSocketBuilder::new(StringConverter)
                                .with_middleware(EchoMiddleware)
                                .with_channel_size(50)
                                .build(),
                        )
                    })
                    .collect();

                // Just return the handlers to ensure they're not optimized away
                handlers
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_handler_creation,
    bench_middleware_chain,
    bench_concurrent_handlers
);
criterion_main!(benches);
