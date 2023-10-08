use async_trait::async_trait;
use coerce::actor::{
    context::ActorContext,
    message::{Handler, Message},
    new_actor, Actor,
};
use criterion::{criterion_group, criterion_main, Criterion};

pub struct PingActor {
    count: usize,
}

#[async_trait]
impl Actor for PingActor {}

pub struct PingMessage(usize);

impl Message for PingMessage {
    type Result = usize;
}

#[async_trait]
impl Handler<PingMessage> for PingActor {
    async fn handle(&mut self, message: PingMessage, _ctx: &mut ActorContext) -> usize {
        self.count += message.0;
        self.count
    }
}

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut ping_address: Option<_> = None; // Zakładam, że znasz typ `YourAddressType`
    runtime
        .block_on(async { ping_address = Some(new_actor(PingActor { count: 10 }).await.unwrap()) });
    c.bench_function("coerce ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    let _res = ping_address
                        .as_ref()
                        .unwrap()
                        .send(PingMessage(10))
                        .await
                        .unwrap();
                }
            })
        })
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
