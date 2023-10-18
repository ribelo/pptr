use criterion::{criterion_group, criterion_main, Criterion};
use tokio::sync::{mpsc, oneshot};

struct PingActor {
    receiver: mpsc::Receiver<PingMessage>,
    count: usize,
}
struct PingMessage {
    count: usize,
    res_tx: Option<oneshot::Sender<usize>>,
}

impl PingActor {
    fn new(receiver: mpsc::Receiver<PingMessage>) -> Self {
        PingActor {
            receiver,
            count: 10,
        }
    }
    async fn handle_message(&mut self, msg: PingMessage) {
        self.count += msg.count;
        if let Some(reply_address) = msg.res_tx {
            reply_address.send(self.count).unwrap();
        }
    }
}

async fn run_my_actor(mut actor: PingActor) {
    while let Some(msg) = actor.receiver.recv().await {
        actor.handle_message(msg).await;
    }
}

#[derive(Clone)]
pub struct PingActorHandle {
    sender: mpsc::Sender<PingMessage>,
}

impl Default for PingActorHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl PingActorHandle {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(8);
        let actor = PingActor::new(receiver);
        tokio::spawn(run_my_actor(actor));

        Self { sender }
    }

    pub async fn send(&self) {
        let msg = PingMessage {
            count: 10,
            res_tx: None,
        };
        self.sender.send(msg).await.unwrap();
    }
    pub async fn ask(&self) -> usize {
        let (res_tx, res_rx) = oneshot::channel();
        let msg = PingMessage {
            count: 10,
            res_tx: Some(res_tx),
        };
        self.sender.send(msg).await.unwrap();
        res_rx.await.expect("Actor task has been killed")
    }
}

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("tokio send ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (sender, receiver) = mpsc::channel(8);
                let actor = PingActor::new(receiver);
                tokio::spawn(run_my_actor(actor));
                let handle = PingActorHandle { sender };
                for _ in 0..100_000 {
                    handle.send().await;
                }
            })
        })
    });
    c.bench_function("tokio ask ping 1e5", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (sender, receiver) = mpsc::channel(8);
                let actor = PingActor::new(receiver);
                tokio::spawn(run_my_actor(actor));
                let handle = PingActorHandle { sender };
                for _ in 0..100_000 {
                    let _r = handle.ask().await;
                }
            })
        })
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
