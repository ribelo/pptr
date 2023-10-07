use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use minions::{
    context::{with_context, with_context_mut},
    gru::{ask, instance_exists, spawn},
    message::Message,
    minion::Minion,
};
use minions_derive::Message;

#[derive(Debug, Clone, Message)]
#[message(response = i32)]
pub struct TestMessage {
    i: i32,
}

#[derive(Debug, Default, Clone)]
pub struct Minion1 {
    i: i32,
}

#[async_trait]
impl Minion for Minion1 {
    type Msg = TestMessage;
    async fn handle_message(
        &mut self,
        msg: TestMessage,
    ) -> <<Self as Minion>::Msg as Message>::Response {
        self.i += 1;
        self.i
    }
}

#[derive(Debug, Default, Clone)]
pub struct Minion2 {
    i: i32,
}

#[async_trait]
impl Minion for Minion2 {
    type Msg = TestMessage;
    async fn handle_message(
        &mut self,
        _msg: TestMessage,
    ) -> <<Self as Minion>::Msg as Message>::Response {
        with_context_mut(|ctx: Option<&mut i32>| {
            if let Some(ctx) = ctx {
                *ctx += 1;
            };
        });
        self.i
    }
}

fn sell_call_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        spawn(Minion1 { i: 0 }).unwrap();
    });
    c.bench_function("actor 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    let _i = ask::<Minion1>(TestMessage { i: 0 }).await.unwrap();
                }
            })
        })
    });
    c.bench_function("with_context_mut 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    let _i = ask::<Minion1>(TestMessage { i: 0 }).await.unwrap();
                }
            })
        })
    });
}

criterion_group!(benches, sell_call_benchmark);
criterion_main!(benches);
