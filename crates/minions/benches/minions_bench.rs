use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use minions::{
    address::Address,
    context::{with_context, with_context_mut},
    gru::{ask, instance_exists, kill, send, spawn, terminate},
    message::Message,
    minion::{Minion, MinionStruct},
};
use minions_derive::Message;

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
    c.bench_function("minion send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    send::<PingActor>(PingMessage(10)).await.unwrap();
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion address send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 });
                let address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    address.send(PingMessage(10)).await.unwrap();
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = ask::<PingActor>(PingMessage(10)).await;
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion ask ping 1e5 bounded 1", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 }).with_buffer_size(1);
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = ask::<PingActor>(PingMessage(10)).await;
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion ask ping 1e5 bounded 10", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 }).with_buffer_size(10);
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = ask::<PingActor>(PingMessage(10)).await;
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion ask ping 1e5 bounded 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 }).with_buffer_size(100_000);
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = ask::<PingActor>(PingMessage(10)).await;
                }
                kill::<PingActor>().await.unwrap();
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
