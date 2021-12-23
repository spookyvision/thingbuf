use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::{runtime, task};

/// This benchmark simulates sending a bunch of strings over a channel. It's
/// intended to simulate the sort of workload that a `thingbuf` is intended
/// for, where the type of element in the buffer is expensive to allocate,
/// copy, or drop, but they can be re-used in place without
/// allocating/deallocating.
///
/// So, this may not be strictly representative of performance in the case of,
/// say, sending a bunch of integers over the channel; instead it simulates
/// the kind of scenario that `thingbuf` is optimized for.
fn bench_mpsc_reusable(c: &mut Criterion) {
    let mut group = c.benchmark_group("async/mpsc_reusable");
    static THE_STRING: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\
aaaaaaaaaaaaaa";

    const SIZE: u64 = 200;
    const CAPACITY: usize = 50;

    for senders in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("ThingBuf", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| async {
                    use thingbuf::{mpsc, ThingBuf};
                    let (tx, rx) = mpsc::channel(ThingBuf::<String>::new(CAPACITY));
                    for _ in 0..senders {
                        let tx = tx.clone();
                        task::spawn(async move {
                            while let Ok(mut slot) = tx.send_ref().await {
                                slot.clear();
                                slot.push_str(THE_STRING);
                            }
                        });
                    }

                    for _ in 0..SIZE {
                        let val = rx.recv_ref().await.unwrap();
                        criterion::black_box(&*val);
                    }
                })
            },
        );

        #[cfg(feature = "futures")]
        group.bench_with_input(
            BenchmarkId::new("futures::channel::mpsc", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| async {
                    use futures::{channel::mpsc, sink::SinkExt, stream::StreamExt};
                    let (tx, mut rx) = mpsc::channel(CAPACITY);
                    for _ in 0..senders {
                        let mut tx = tx.clone();
                        task::spawn(async move {
                            while tx.send(String::from(THE_STRING)).await.is_ok() {}
                        });
                    }
                    for _ in 0..SIZE {
                        let val = rx.next().await.unwrap();
                        criterion::black_box(&val);
                    }
                })
            },
        );

        #[cfg(feature = "tokio-sync")]
        group.bench_with_input(
            BenchmarkId::new("tokio::sync::mpsc", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| {
                    // turn off Tokio's automatic cooperative yielding for this
                    // benchmark. in code with a large number of concurrent
                    // tasks, this feature makes the MPSC channel (and other
                    // Tokio synchronization primitives) better "team players"
                    // than other implementations, since it prevents them from
                    // using too much scheduler time.
                    //
                    // in this benchmark, though, there *are* no other tasks
                    // running, so automatic yielding just means we spend more
                    // time ping-ponging through the scheduler than every other
                    // implementation.
                    tokio::task::unconstrained(async {
                        use tokio::sync::mpsc;
                        let (tx, mut rx) = mpsc::channel(CAPACITY);

                        for _ in 0..senders {
                            let tx = tx.clone();
                            task::spawn(tokio::task::unconstrained(async move {
                                // this actually brings Tokio's MPSC closer to what
                                // `ThingBuf` can do than all the other impls --- we
                                // only allocate if we _were_ able to reserve send
                                // capacity. but, we will still allocate and
                                // deallocate a string for every message...
                                while let Ok(permit) = tx.reserve().await {
                                    permit.send(String::from(THE_STRING));
                                }
                            }));
                        }
                        for _ in 0..SIZE {
                            let val = rx.recv().await.unwrap();
                            criterion::black_box(&val);
                        }
                    })
                })
            },
        );

        #[cfg(feature = "async-std")]
        group.bench_with_input(
            BenchmarkId::new("async_std::channel::bounded", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| async {
                    use async_std::channel;
                    let (tx, rx) = channel::bounded(CAPACITY);

                    for _ in 0..senders {
                        let tx = tx.clone();
                        task::spawn(async move {
                            while tx.send(String::from(THE_STRING)).await.is_ok() {}
                        });
                    }
                    for _ in 0..SIZE {
                        let val = rx.recv().await.unwrap();
                        criterion::black_box(&val);
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_mpsc_integer(c: &mut Criterion) {
    let mut group = c.benchmark_group("async/mpsc_integer");

    const SIZE: u64 = 1_000;
    const CAPACITY: usize = 100;

    for senders in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("ThingBuf", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| async {
                    use thingbuf::{mpsc, ThingBuf};
                    let (tx, rx) = mpsc::channel(ThingBuf::new(CAPACITY));
                    for i in 0..senders {
                        let tx = tx.clone();
                        task::spawn(async move {
                            while let Ok(mut slot) = tx.send_ref().await {
                                *slot = i;
                            }
                        });
                    }

                    for _ in 0..SIZE {
                        let val = rx.recv_ref().await.unwrap();
                        criterion::black_box(&*val);
                    }
                })
            },
        );

        #[cfg(feature = "futures")]
        group.bench_with_input(
            BenchmarkId::new("futures::channel::mpsc", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| async {
                    use futures::{channel::mpsc, sink::SinkExt, stream::StreamExt};
                    let (tx, mut rx) = mpsc::channel(CAPACITY);
                    for i in 0..senders {
                        let mut tx = tx.clone();
                        task::spawn(async move { while tx.send(i).await.is_ok() {} });
                    }
                    for _ in 0..SIZE {
                        let val = rx.next().await.unwrap();
                        criterion::black_box(&val);
                    }
                })
            },
        );

        #[cfg(feature = "tokio-sync")]
        group.bench_with_input(
            BenchmarkId::new("tokio::sync::mpsc", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| {
                    // turn off Tokio's automatic cooperative yielding for this
                    // benchmark. in code with a large number of concurrent
                    // tasks, this feature makes the MPSC channel (and other
                    // Tokio synchronization primitives) better "team players"
                    // than other implementations, since it prevents them from
                    // using too much scheduler time.
                    //
                    // in this benchmark, though, there *are* no other tasks
                    // running, so automatic yielding just means we spend more
                    // time ping-ponging through the scheduler than every other
                    // implementation.
                    tokio::task::unconstrained(async {
                        use tokio::sync::mpsc;
                        let (tx, mut rx) = mpsc::channel(CAPACITY);

                        for i in 0..senders {
                            let tx = tx.clone();
                            task::spawn(tokio::task::unconstrained(async move {
                                while tx.send(i).await.is_ok() {}
                            }));
                        }
                        for _ in 0..SIZE {
                            let val = rx.recv().await.unwrap();
                            criterion::black_box(&val);
                        }
                    })
                })
            },
        );

        #[cfg(feature = "async-std")]
        group.bench_with_input(
            BenchmarkId::new("async_std::channel::bounded", senders),
            &senders,
            |b, &senders| {
                b.to_async(rt()).iter(|| async {
                    use async_std::channel;
                    let (tx, rx) = channel::bounded(CAPACITY);

                    for i in 0..senders {
                        let tx = tx.clone();
                        task::spawn(async move { while tx.send(i).await.is_ok() {} });
                    }
                    for _ in 0..SIZE {
                        let val = rx.recv().await.unwrap();
                        criterion::black_box(&val);
                    }
                })
            },
        );
    }

    group.finish();
}

fn rt() -> tokio::runtime::Runtime {
    runtime::Builder::new_multi_thread().build().unwrap()
}

criterion_group!(benches, bench_mpsc_reusable, bench_mpsc_integer,);
criterion_main!(benches);
