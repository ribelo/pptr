use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use minions::{
    address::Address,
    context::{with_context, with_context_mut},
    gru::{ask, instance_exists, send, spawn},
    message::Message,
    minion::Minion,
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
    let mut ping_address: Option<Address<_>> = None; // Zakładam, że znasz typ `YourAddressType`
    runtime.block_on(async {
        ping_address = Some(spawn(PingActor { count: 10 }).unwrap());
    });
    c.bench_function("minion send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    send::<PingActor>(PingMessage(10)).await.unwrap();
                }
            })
        })
    });
    c.bench_function("minion address send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let address = ping_address.as_ref().unwrap();
                for _ in 0..100_000 {
                    address.send(PingMessage(10)).await.unwrap();
                }
            })
        })
    });
    c.bench_function("minion ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                for _ in 0..100_000 {
                    let _res = ask::<PingActor>(PingMessage(10)).await;
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
