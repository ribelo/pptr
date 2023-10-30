// use async_trait::async_trait;
// use criterion::{criterion_group, criterion_main, Criterion};
// use minions::{
//     address::PuppetAddress,
//     errors::PuppetError,
//     // context::{with_context, with_context_mut},
//     master::{Master, Puppeter},
//     message::Message,
//     puppet::{execution, Handler, Puppet, PuppetStruct},
// };
//
// #[derive(Clone)]
// struct PingMessage(usize);
//
// impl Message for PingMessage {}
//
// #[derive(Clone, Default)]
// struct PingActor {
//     count: usize,
// }
//
// impl Master for PingActor {}
// impl Puppet for PingActor {}
//
// #[derive(Clone, Default)]
// struct ConcurrentPingActor {
//     count: usize,
// }
//
// impl Master for ConcurrentPingActor {}
// impl Puppet for ConcurrentPingActor {}
//
// #[derive(Clone, Default)]
// struct ParallelPingActor {
//     count: usize,
// }
//
// impl Master for ParallelPingActor {}
// impl Puppet for ParallelPingActor {}
//
// #[async_trait]
// impl Handler<PingMessage> for PingActor {
//     type Response = usize;
//     type Exec = execution::Sequential;
//     async fn handle_message(&mut self, msg: PingMessage) -> Result<usize,
// PuppetError> {         self.count += msg.0;
//         Ok(self.count)
//     }
// }
//
// #[async_trait]
// impl Handler<PingMessage> for ConcurrentPingActor {
//     type Response = usize;
//     type Exec = execution::Concurrent;
//     async fn handle_message(&mut self, msg: PingMessage) -> Result<usize,
// PuppetError> {         self.count += msg.0;
//         Ok(self.count)
//     }
// }
//
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
//
// fn benchmarks(c: &mut Criterion) {
//     let runtime = tokio::runtime::Runtime::new().unwrap();
//     c.bench_function("minion send ping 1e5", |b| {
//         b.iter(|| {
//             runtime.block_on(async {
//                 let puppeter = Puppeter::new();
//                 let actor = PingActor { count: 10 };
//                 let _address = actor.spawn().unwrap();
//                 for i in 0..100_000 {
//                     puppeter
//                         .send::<PingActor, _>(PingMessage(10))
//                         .await
//                         .unwrap();
//                 }
//                 puppeter.kill_puppet::<Puppeter, PingActor>().await.unwrap();
//             })
//         })
//     });
//     c.bench_function("minion address send ping 1e5", |b| {
//         b.iter(|| {
//             runtime.block_on(async {
//                 let puppeter = Puppeter::new();
//                 let actor = PingActor { count: 10 };
//                 let address = actor.spawn().unwrap();
//                 for _ in 0..100_000 {
//                     address.send(PingMessage(10)).await.unwrap();
//                 }
//                 puppeter.kill_puppet::<Puppeter, PingActor>().await.unwrap();
//             })
//         })
//     });
//     c.bench_function("minion ask ping 1e5", |b| {
//         b.iter(|| {
//             runtime.block_on(async {
//                 let puppeter = Puppeter::new();
//                 let actor = PingActor { count: 10 };
//                 let _address = actor.spawn().unwrap();
//                 for _ in 0..100_000 {
//                     puppeter.ask::<PingActor,
// _>(PingMessage(10)).await.unwrap();                 }
//                 puppeter.kill_puppet::<Puppeter, PingActor>().await.unwrap();
//             })
//         })
//     });
//     c.bench_function("minion address ask ping 1e5", |b| {
//         b.iter(|| {
//             runtime.block_on(async {
//                 let puppeter = Puppeter::new();
//                 let actor = PingActor { count: 10 };
//                 let address = actor.spawn().unwrap();
//                 for _ in 0..100_000 {
//                     address.ask(PingMessage(10)).await.unwrap();
//                 }
//                 puppeter.kill_puppet::<Puppeter, PingActor>().await.unwrap();
//             })
//         })
//     });
//     // c.bench_function("minion concurrent send ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ConcurrentPingActor { count:
// 10     // });             let _address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 gru.send::<ConcurrentPingActor, _>(PingMessage(10))
//     //                     .await
//     //                     .unwrap();
//     //             }
//     //             gru.kill::<ConcurrentPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // c.bench_function("minion concurrent address send ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ConcurrentPingActor { count:
// 10     // });             let address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 address.send(PingMessage(10)).await.unwrap();
//     //             }
//     //             gru.kill::<ConcurrentPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // c.bench_function("minion concurrent ask ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ConcurrentPingActor { count:
// 10     // });             let _address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 let _res = gru.ask::<ConcurrentPingActor,
//     // _>(PingMessage(10)).await;             }
//     //             gru.kill::<ConcurrentPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // c.bench_function("minion address concurrent ask ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ConcurrentPingActor { count:
// 10     // });             let address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 let _res = address.ask(PingMessage(10)).await;
//     //             }
//     //             gru.kill::<ConcurrentPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // #[cfg(feature = "rayon")]
//     // c.bench_function("minion parallel send ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ParallelPingActor { count:
// 10     // });             let _address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 gru.send::<ParallelPingActor, _>(PingMessage(10))
//     //                     .await
//     //                     .unwrap();
//     //             }
//     //             gru.kill::<ParallelPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // #[cfg(feature = "rayon")]
//     // c.bench_function("minion parallel address send ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ParallelPingActor { count:
// 10     // });             let address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 address.send(PingMessage(10)).await.unwrap();
//     //             }
//     //             gru.kill::<ParallelPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // #[cfg(feature = "rayon")]
//     // c.bench_function("minion parallel ask ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ParallelPingActor { count:
// 10     // });             let _address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 let _res = gru.ask::<ParallelPingActor,
//     // _>(PingMessage(10)).await;             }
//     //             gru.kill::<ParallelPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // #[cfg(feature = "rayon")]
//     // c.bench_function("minion address parallel ask ping 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let gru = Puppeter::new();
//     //             let actor = PuppetStruct::new(ParallelPingActor { count:
// 10     // });             let address = gru.spawn(actor).unwrap();
//     //             for _ in 0..100_000 {
//     //                 let _res = address.ask(PingMessage(10)).await;
//     //             }
//     //             gru.kill::<ParallelPingActor>().await.unwrap();
//     //         })
//     //     })
//     // });
//     // c.bench_function("with_context_mut 1e5", |b| {
//     //     b.iter(|| {
//     //         runtime.block_on(async {
//     //             let address = address.as_ref().unwrap();
//     //             let msg = TestMessage { i: 0 };
//     //             for _ in 0..100_000 {
//     //                 let _i = address.ask(msg.clone()).await.unwrap();
//     //             }
//     //         })
//     //     })
//     // });
// }
//
// criterion_group!(benches, benchmarks);
// criterion_main!(benches);
