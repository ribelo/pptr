use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use minions::{
    address::Address,
    // context::{with_context, with_context_mut},
    gru::{ask, kill, minion_exists, send, spawn, stop, Gru},
    message::Message,
    minion::{execution, Handler, Minion, MinionStruct},
};

#[derive(Clone)]
struct PingMessage(usize);

impl Message for PingMessage {}

#[derive(Clone)]
struct PingActor {
    count: usize,
}

impl Minion for PingActor {}

#[derive(Clone)]
struct ConcurrentPingActor {
    count: usize,
}

impl Minion for ConcurrentPingActor {}

#[derive(Clone)]
struct ParallelPingActor {
    count: usize,
}

impl Minion for ParallelPingActor {}

#[async_trait]
impl Handler<PingMessage> for PingActor {
    type Response = usize;
    type Exec = execution::Sequential;
    async fn handle_message(&mut self, msg: &PingMessage) -> usize {
        self.count += msg.0;
        self.count
    }
}

#[async_trait]
impl Handler<PingMessage> for ConcurrentPingActor {
    type Response = usize;
    type Exec = execution::Concurrent;
    async fn handle_message(&mut self, msg: &PingMessage) -> usize {
        self.count += msg.0;
        self.count
    }
}

#[async_trait]
impl Handler<PingMessage> for ParallelPingActor {
    type Response = usize;
    type Exec = execution::Parallel;
    async fn handle_message(&mut self, msg: &PingMessage) -> usize {
        self.count += msg.0;
        self.count
    }
}

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let gru = Gru::new();
    c.bench_function("minion send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                // println!("before start");
                let actor = MinionStruct::new(PingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    gru.send::<PingActor, _>(PingMessage(10)).await.unwrap();
                }
                kill::<PingActor>().await.unwrap();
                // println!("after kill");
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
                    let _res = ask::<PingActor, _>(PingMessage(10)).await;
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion address ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(PingActor { count: 10 });
                let address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = address.ask(PingMessage(10)).await;
                }
                kill::<PingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion concurrent send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(ConcurrentPingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    send::<ConcurrentPingActor, _>(PingMessage(10))
                        .await
                        .unwrap();
                }
                kill::<ConcurrentPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion concurrent address send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(ConcurrentPingActor { count: 10 });
                let address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    address.send(PingMessage(10)).await.unwrap();
                }
                kill::<ConcurrentPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion concurrent ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let actor = MinionStruct::new(ConcurrentPingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = ask::<ConcurrentPingActor, _>(PingMessage(10)).await;
                }
                kill::<ConcurrentPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion address concurrent ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let _gru = Gru::new();
                let actor = MinionStruct::new(ParallelPingActor { count: 10 });
                let address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = address.ask(PingMessage(10)).await;
                }
                kill::<ParallelPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion parallel send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let _gru = Gru::new();
                let actor = MinionStruct::new(ParallelPingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    send::<ParallelPingActor, _>(PingMessage(10)).await.unwrap();
                }
                kill::<ParallelPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion parallel address send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let _gru = Gru::new();
                let actor = MinionStruct::new(ParallelPingActor { count: 10 });
                let address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    address.send(PingMessage(10)).await.unwrap();
                }
                kill::<ParallelPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion parallel ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let _gru = Gru::new();
                let actor = MinionStruct::new(ParallelPingActor { count: 10 });
                let _address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = ask::<ParallelPingActor, _>(PingMessage(10)).await;
                }
                kill::<ParallelPingActor>().await.unwrap();
            })
        })
    });
    c.bench_function("minion address parallel ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let _gru = Gru::new();
                let actor = MinionStruct::new(ParallelPingActor { count: 10 });
                let address = spawn(actor).unwrap();
                for _ in 0..100_000 {
                    let _res = address.ask(PingMessage(10)).await;
                }
                kill::<ParallelPingActor>().await.unwrap();
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
