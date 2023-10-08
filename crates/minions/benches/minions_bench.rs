use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use minions::{
    address::Address,
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

#[derive(Clone, Message)]
#[message(response = usize)]
struct PingMessage(usize);

struct PingActor {
    count: usize,
}

#[async_trait]
impl Minion for PingActor {
    type Msg = PingMessage;
    async fn handle_message(&mut self, msg: PingMessage) -> usize {
        self.count += msg.0;
        self.count
    }
}

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut minion = Minion1 { i: 0 };
    let mut address: Option<Address<_>> = None; // Zakładam, że znasz typ `YourAddressType`
    runtime.block_on(async {
        address = Some(spawn(minion.clone()).unwrap());
        spawn(PingActor { count: 10 }).unwrap();
    });
    c.bench_function("function call 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let msg = TestMessage { i: 0 };
                for _ in 0..100_000 {
                    let _i = minion.handle_message(msg.clone()).await;
                }
            })
        })
    });
    c.bench_function("minion ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    let _res = ask::<PingActor>(PingMessage(10)).await;
                }
            })
        })
    });
    c.bench_function("actor 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let msg = TestMessage { i: 0 };
                for _ in 0..100_000 {
                    let _i = ask::<Minion1>(msg.clone()).await.unwrap();
                }
            })
        })
    });
    c.bench_function("address 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let msg = TestMessage { i: 0 };
                let address = address.as_ref().unwrap();
                for _ in 0..100_000 {
                    let _i = address.ask(msg.clone()).await.unwrap();
                }
            })
        })
    });
    // c.bench_function("with_context_mut 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let address = address.as_ref().unwrap();
    //             let msg = TestMessage { i: 0 };
    //             for _ in 0..100_000 {
    //                 let _i = address.ask(msg.clone()).await.unwrap();
    //             }
    //         })
    //     })
    // });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
