use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use tokio::sync::{mpsc, oneshot};

fn benchmarks(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("primitives std mutex read 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use std::sync::Mutex;
                let m = Arc::new(Mutex::new(0));
                let mut set = tokio::task::JoinSet::new();

                for _ in 0..1000 {
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            let mut num = m.lock().unwrap();
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives std rwlock read 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use std::sync::RwLock;
                let m = Arc::new(RwLock::new(0));
                let mut set = tokio::task::JoinSet::new();

                for _ in 0..1000 {
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            let mut num = m.read().unwrap();
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives parking_lot mutex read 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use parking_lot::Mutex;
                let m = Arc::new(Mutex::new(0));
                let mut set = tokio::task::JoinSet::new();

                for _ in 0..1000 {
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            let mut num = m.lock();
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives parking_lot rwlock read 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use parking_lot::RwLock;
                let m = Arc::new(RwLock::new(0));
                let mut set = tokio::task::JoinSet::new();

                for _ in 0..1000 {
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            let mut num = m.read();
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives std mutex mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use std::sync::Mutex;
                let m = Arc::new(Mutex::new(0));
                let mut set = tokio::task::JoinSet::new();

                let mut switch = false;
                for _ in 0..1000 {
                    switch = !switch;
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            match switch {
                                true => {
                                    let mut num = m.lock().unwrap();
                                }
                                false => {
                                    let mut num = m.lock().unwrap();
                                    *num += 1;
                                }
                            }
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives std rwlock mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use std::sync::RwLock;
                let m = Arc::new(RwLock::new(0));
                let mut set = tokio::task::JoinSet::new();

                let mut switch = false;
                for _ in 0..1000 {
                    switch = !switch;
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            match switch {
                                true => {
                                    let mut num = m.read().unwrap();
                                }
                                false => {
                                    let mut num = m.write().unwrap();
                                    *num += 1;
                                }
                            }
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives parking_lot mutex mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use parking_lot::Mutex;
                let m = Arc::new(Mutex::new(0));
                let mut set = tokio::task::JoinSet::new();

                let mut switch = false;
                for _ in 0..1000 {
                    switch = !switch;
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            match switch {
                                true => {
                                    let mut num = m.lock();
                                }
                                false => {
                                    let mut num = m.lock();
                                    *num += 1;
                                }
                            }
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives parking_lot rwlock mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                use parking_lot::RwLock;
                let m = Arc::new(RwLock::new(0));
                let mut set = tokio::task::JoinSet::new();

                let mut switch = false;
                for _ in 0..1000 {
                    switch = !switch;
                    let m = Arc::clone(&m);
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            match switch {
                                true => {
                                    let mut num = m.read();
                                }
                                false => {
                                    let mut num = m.write();
                                    *num += 1;
                                }
                            }
                        }
                    });
                }

                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives tokio mpsc mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (tx, mut rx) = mpsc::channel::<i32>(100);
                let mut set = tokio::task::JoinSet::new();
                let threads = 10;
                let messages = 1000;
                let total_messages = theads * messages; // 100 tasks * 100_000 messages each
                let mut received_messages = 0;

                for _ in 0..threads {
                    let tx = tx.clone();
                    let handle = set.spawn(async move {
                        for _ in 0..messages {
                            tx.send(1).await.unwrap();
                        }
                    });
                }

                // Create a task to receive messages
                let receive_handle = set.spawn(async move {
                    while received_messages < total_messages {
                        let _received = rx.recv().await.unwrap();
                        received_messages += 1;
                    }
                });

                // Wait for all tasks to complete
                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives flume mpsc mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (tx, mut rx) = flume::bounded::<i32>(100);
                let mut set = tokio::task::JoinSet::new();
                let threads = 100;
                let messages = 1000;
                let total_messages = theads * messages; // 100 tasks * 100_000 messages each
                let mut received_messages = 0;

                for _ in 0..threads {
                    let tx = tx.clone();
                    let handle = set.spawn(async move {
                        for _ in 0..messages {
                            tx.send_async(1).await.unwrap();
                        }
                    });
                }

                // Create a task to receive messages
                let receive_handle = set.spawn(async move {
                    while received_messages < total_messages {
                        let _received = rx.recv_async().await.unwrap();
                        received_messages += 1;
                    }
                });

                // Wait for all tasks to complete
                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives kanal async mpsc mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (tx, mut rx) = kanal::bounded::<i32>(100);
                let mut set = tokio::task::JoinSet::new();
                let threads = 100;
                let messages = 1000;
                let total_messages = theads * messages; // 100 tasks * 100_000 messages each

                for _ in 0..threads {
                    let tx = tx.clone();
                    let handle = set.spawn(async move {
                        for _ in 0..messages {
                            tx.send(1).unwrap();
                        }
                    });
                }

                // Create a task to receive messages
                let receive_handle = set.spawn(async move {
                    while received_messages < total_messages {
                        let _received = rx.recv().unwrap();
                        received_messages += 1;
                    }
                });

                // Wait for all tasks to complete
                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives kanal sync mpsc mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (tx, mut rx) = kanal::bounded::<i32>(100);
                let mut set = tokio::task::JoinSet::new();
                let threads = 100;
                let messages = 1000;
                let total_messages = theads * messages; // 100 tasks * 100_000 messages each
                let mut received_messages = 0;

                for _ in 0..10 {
                    let tx = tx.clone();
                    let handle = set.spawn(async move {
                        for _ in 0..1000 {
                            tx.send(1).unwrap();
                        }
                    });
                }

                // Create a task to receive messages
                let receive_handle = set.spawn(async move {
                    while received_messages < total_messages {
                        let _received = rx.recv().unwrap();
                        received_messages += 1;
                    }
                });

                // Wait for all tasks to complete
                while let Some(handle) = set.join_next().await {
                    handle.unwrap();
                }
            })
        })
    });
    c.bench_function("primitives crossbeam mpsc mixed 1e3", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let (tx, mut rx) = crossbeam::channel::bounded::<i32>(100);
                let total_messages = 1000 * 1000; // 100 tasks * 100_000 messages each
                let mut received_messages = 0;
                let mut handles = vec![];

                for _ in 0..10 {
                    let tx = tx.clone();
                    let handle = std::thread::spawn(move || {
                        for _ in 0..1000 {
                            tx.send(1).unwrap();
                        }
                    });
                    handles.push(handle);
                }

                // Create a task to receive messages
                for handle in handles {
                    handle.join().unwrap();
                }

                // Wait for all tasks to complete
            })
        })
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
