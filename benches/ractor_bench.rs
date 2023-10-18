use criterion::{criterion_group, criterion_main, Criterion};
use ractor::{async_trait, cast, Actor, ActorProcessingErr, ActorRef};

#[derive(Debug, Clone)]
pub struct PingMessage(usize);

pub struct PingActor;

#[async_trait]
impl Actor for PingActor {
    type Msg = PingMessage;

    type State = usize;
    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(10)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        *state += message.0;
        Ok(())
    }
}

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("ractor send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (actor, handle) = Actor::spawn(None, PingActor, ()).await.unwrap();
                for _ in 0..100_000 {
                    actor.send_message(PingMessage(10)).unwrap();
                }
                actor.stop(None);
                handle.await.unwrap();
            })
        })
    });
    // c.bench_function("ractor ask ping 1e5", |b| {
    //     b.iter(|| {
    //         runtime.block_on(async {
    //             let (actor, handle) = std::mem::take(&mut ping_address).unwrap();
    //             for _ in 0..100_000 {
    //                 actor.send_message(PingMessage(10)).unwrap();
    //             }
    //             handle.await.unwrap();
    //         })
    //     })
    // });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
