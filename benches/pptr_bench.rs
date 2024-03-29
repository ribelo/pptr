use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use pptr::{
    errors::PuppetError,
    executor::{ConcurrentExecutor, SequentialExecutor},
    message::Message,
    puppet::{Handler, Lifecycle, PuppetBuilder},
    supervision::strategy::OneForAll,
};

// #[derive(Clone, Debug)]
// struct PingMessage(usize);
//
// impl Message for PingMessage {}
//
// #[derive(Clone, Default)]
// struct PingActor {
//     count: usize,
// }
//
// impl Lifecycle for PingActor {
//     type Supervision = OneForAll;
// }
//
// #[derive(Clone, Default)]
// struct ConcurrentPingActor {
//     count: usize,
// }
//
// impl Lifecycle for ConcurrentPingActor {
//     type Supervision = OneForAll;
// }
//
// #[derive(Clone, Default)]
// struct ParallelPingActor {
//     count: usize,
// }
//
// // impl Lifecycle for ParallelPingActor {}
//
// #[async_trait]
// impl Handler<PingMessage> for PingActor {
//     type Response = usize;
//     type Executor = SequentialExecutor;
//     async fn handle_message(
//         &mut self,
//         msg: PingMessage,
//         puppeter: &Puppeter,
//     ) -> Result<usize, PuppetError> {
//         self.count += msg.0;
//         Ok(self.count)
//     }
// }
//
// #[async_trait]
// impl Handler<PingMessage> for ConcurrentPingActor {
//     type Response = usize;
//     type Executor = ConcurrentExecutor;
//     async fn handle_message(
//         &mut self,
//         msg: PingMessage,
//         puppeter: &Puppeter,
//     ) -> Result<usize, PuppetError> {
//         self.count += msg.0;
//         Ok(self.count)
//     }
// }

// #[cfg(feature = "rayon")]
// #[async_trait]
// impl Handler<PingMessage> for ParallelPingActor {
//     type Response = usize;
//     type Exec = execution::Parallel;
//     async fn handle_message(&mut self, msg: PingMessage) -> Result<usize,
// PuppetError> {         self.count += msg.0;
//         Ok(self.count)
//     }
// }

fn benchmarks(c: &mut Criterion) {
    // let runtime = tokio::runtime::Runtime::new().unwrap();
    // c.bench_function("minion send ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let post_office = MasterOfPuppets::new();
    //             let actor =
    //                 PuppetBuilder::new(PingActor { count: 10 }).with_post_office(&post_office);
    //
    //             let _address = actor.spawn().await.unwrap();
    //             for i in 0..100_000 {
    //                 post_office
    //                     .send::<PingActor, _>(PingMessage(10))
    //                     .await
    //                     .unwrap();
    //             }
    //         })
    //     })
    // });
    // c.bench_function("minion address send ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let post_office = MasterOfPuppets::new();
    //             let actor =
    //                 PuppetBuilder::new(PingActor { count: 10 }).with_post_office(&post_office);
    //             let address = actor.spawn().await.unwrap();
    //             for _ in 0..100_000 {
    //                 address.send(PingMessage(10)).await.unwrap();
    //             }
    //         })
    //     })
    // });
    // c.bench_function("minion ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let post_office = MasterOfPuppets::new();
    //             let actor =
    //                 PuppetBuilder::new(PingActor { count: 10 }).with_post_office(&post_office);
    //             let address = actor.spawn().await.unwrap();
    //             for _ in 0..100_000 {
    //                 post_office
    //                     .ask::<PingActor, _>(PingMessage(10))
    //                     .await
    //                     .unwrap();
    //             }
    //         })
    //     })
    // });
    // c.bench_function("minion address ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let post_office = MasterOfPuppets::new();
    //             let actor =
    //                 PuppetBuilder::new(PingActor { count: 10 }).with_post_office(&post_office);
    //             let address = actor.spawn().await.unwrap();
    //             for _ in 0..100_000 {
    //                 address.ask(PingMessage(10)).await.unwrap();
    //             }
    //         })
    //     })
    // });
    // c.bench_function("minion concurrent send ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ConcurrentPingActor { count 10     // });             let _address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 gru.send::<ConcurrentPingActor, _>(PingMessage(10))
    //                     .await
    //                     .unwrap();
    //             }
    //             gru.kill::<ConcurrentPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // c.bench_function("minion concurrent address send ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ConcurrentPingActor { count: 10     // });             let address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 address.send(PingMessage(10)).await.unwrap();
    //             }
    //             gru.kill::<ConcurrentPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // c.bench_function("minion concurrent ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ConcurrentPingActor { count: 10     // });             let _address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 let _res = gru.ask::<ConcurrentPingActor,
    // _>(PingMessage(10)).await;             }
    //             gru.kill::<ConcurrentPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // c.bench_function("minion address concurrent ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ConcurrentPingActor { count: 10     // });             let address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 let _res = address.ask(PingMessage(10)).await;
    //             }
    //             gru.kill::<ConcurrentPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // #[cfg(feature = "rayon")]
    // c.bench_function("minion parallel send ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ParallelPingActor { count: 10     // });             let _address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 gru.send::<ParallelPingActor, _>(PingMessage(10))
    //                     .await
    //                     .unwrap();
    //             }
    //             gru.kill::<ParallelPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // #[cfg(feature = "rayon")]
    // c.bench_function("minion parallel address send ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ParallelPingActor { count: 10     // });             let address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 address.send(PingMessage(10)).await.unwrap();
    //             }
    //             gru.kill::<ParallelPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // #[cfg(feature = "rayon")]
    // c.bench_function("minion parallel ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ParallelPingActor { count: 10     // });             let _address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 let _res = gru.ask::<ParallelPingActor,
    // _>(PingMessage(10)).await;             }
    //             gru.kill::<ParallelPingActor>().await.unwrap();
    //         })
    //     })
    // });
    // #[cfg(feature = "rayon")]
    // c.bench_function("minion address parallel ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let gru = Puppeter::new();
    //             let actor = PuppetStruct::new(ParallelPingActor { count: 10     // });             let address = gru.spawn(actor).unwrap();
    //             for _ in 0..100_000 {
    //                 let _res = address.ask(PingMessage(10)).await;
    //             }
    //             gru.kill::<ParallelPingActor>().await.unwrap();
    //         })
    //     })
    // });
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
