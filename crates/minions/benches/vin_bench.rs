use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use vin::prelude::*;

#[vin::message]
#[derive(Debug, Clone)]
struct PingMessage(usize);

#[vin::actor]
#[vin::handles(PingMessage, max = 128)]
struct PingActor {
    count: usize,
}

impl vin::Hooks for PingActor {}

#[async_trait]
impl vin::Handler<PingMessage> for PingActor {
    async fn handle(&self, msg: PingMessage) -> Result<(), ()> {
        let mut ctx = self.ctx_mut().await;
        ctx.count += msg.0;
        Ok(())
    }
}

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut a: Option<_> = None;
    runtime.block_on(async {
        a = Some(PingActor::start("test1", VinContextPingActor { count: 10 }).unwrap());
    });
    c.bench_function("vin send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    a.as_ref().unwrap().send(PingMessage(10)).await;
                }
            })
        })
    });
    c.bench_function("vin ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    let _res = a.as_ref().unwrap().send_and_wait(PingMessage(10)).await;
                }
            })
        })
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
