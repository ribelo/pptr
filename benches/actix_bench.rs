use actix::prelude::*;
use criterion::{criterion_group, criterion_main, Criterion};

/// Define `Ping` message
struct Ping(usize);

impl Message for Ping {
    type Result = usize;
}

/// Actor
struct MyActor {
    count: usize,
}

/// Declare actor and its context
impl Actor for MyActor {
    type Context = Context<Self>;
}

/// Handler for `Ping` message
impl Handler<Ping> for MyActor {
    type Result = usize;

    fn handle(&mut self, msg: Ping, _: &mut Context<Self>) -> Self::Result {
        self.count += msg.0;
        self.count
    }
}

fn benchmarks(c: &mut Criterion) {
    let sys = System::new();
    c.bench_function("actix ping 1e5", |b| {
        b.iter(|| {
            sys.block_on(async {
                let addr = MyActor { count: 10 }.start();
                for _ in 0..100_000 {
                    let _res = addr.send(Ping(10)).await;
                }
            });
        })
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
