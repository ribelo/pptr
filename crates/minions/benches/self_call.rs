use async_trait::async_trait;
use criterion::{criterion_group, criterion_main, Criterion};
use ox_frame::{actor::Minion, dispatcher::Dispatcher, message::Message, ox_frame::OxFrame};

#[derive(Debug, Clone)]
pub struct TestMessage {
    i: i32,
}

impl Message for TestMessage {
    type Response = i32;
}

#[derive(Debug, Default, Clone)]
pub struct TestActor {
    i: i32,
}

#[async_trait]
impl Minion for TestActor {
    type Msg = TestMessage;
    async fn handle_message(
        &mut self,
        ox: OxFrame,
        msg: TestMessage,
    ) -> <<Self as Minion>::Msg as Message>::Response {
        self.i += 1;
        self.i
    }
}

fn sell_call_benchmark(c: &mut Criterion) {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("actor 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let ox_frame = OxFrame::new();

                ox_frame.spawn(TestActor { i: 0 }).unwrap();
                for _ in 0..100_000 {
                    let _i = ox_frame
                        .ask::<TestActor>(TestMessage { i: 0 })
                        .await
                        .unwrap();
                }
            })
        })
    });
}

criterion_group!(benches, sell_call_benchmark);
criterion_main!(benches);
