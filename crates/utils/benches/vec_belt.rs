use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Bencher, BenchmarkId, Criterion, Throughput};
use hephae_utils::{sync::*, vec_belt::VecBelt};

const APPEND_COUNT: usize = 128;
const DATA_LEN: usize = 128;
const CHUNK_LEN: &[usize] = &[512, 4096, 32768];
const THREAD_COUNT: &[usize] = &[4, 16, 64];

struct Join(Option<thread::JoinHandle<()>>);
impl Drop for Join {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.join().unwrap();
        }
    }
}

fn bench_vec_mutex(bench: &mut Bencher, thread_count: usize, append_count: usize, chunk_len: usize) {
    let main_thread = thread::current();
    bench.iter_batched_ref(
        || {
            let data = Arc::new(Mutex::new(Vec::<usize>::with_capacity(chunk_len)));
            let signal = Arc::new((AtomicBool::new(false), AtomicUsize::new(thread_count)));
            let threads = (0..thread_count)
                .map(|i| {
                    let input = (0..DATA_LEN).map(|num| num + i * chunk_len).collect::<Box<_>>();
                    let main_thread = main_thread.clone();
                    let data = data.clone();
                    let signal = signal.clone();

                    Join(
                        thread::Builder::new()
                            .stack_size(1024)
                            .spawn(move || {
                                let (begin, end) = &*signal;
                                while !begin.load(Relaxed) {
                                    thread::yield_now();
                                }

                                for _ in 0..append_count {
                                    data.lock().unwrap().extend_from_slice(&input);
                                }

                                drop(data);
                                if end.fetch_sub(1, Release) == 1 {
                                    main_thread.unpark();
                                }
                            })
                            .unwrap()
                            .into(),
                    )
                })
                .collect::<Box<_>>();

            (data, signal, threads)
        },
        |(data, signal, ..)| {
            let (begin, end) = &**signal;
            begin.store(true, Relaxed);

            while end.load(Acquire) > 0 {
                thread::park();
            }

            assert_eq!(
                Arc::get_mut(data).unwrap().get_mut().unwrap().len(),
                thread_count * DATA_LEN * APPEND_COUNT
            );
        },
        BatchSize::SmallInput,
    );
}

fn bench_vec_belt(bench: &mut Bencher, thread_count: usize, append_count: usize, chunk_len: usize) {
    let main_thread = thread::current();
    bench.iter_batched_ref(
        || {
            let data = Arc::new(VecBelt::<usize>::new(chunk_len));
            let signal = Arc::new((AtomicBool::new(false), AtomicUsize::new(thread_count)));
            let threads = (0..thread_count)
                .map(|i| {
                    let input = (0..DATA_LEN).map(|num| num + i * chunk_len).collect::<Box<_>>();
                    let main_thread = main_thread.clone();
                    let data = data.clone();
                    let signal = signal.clone();

                    Join(
                        thread::Builder::new()
                            .stack_size(1024)
                            .spawn(move || {
                                let (begin, end) = &*signal;
                                while !begin.load(Relaxed) {
                                    thread::yield_now();
                                }

                                for _ in 0..append_count {
                                    data.append(&*input);
                                }

                                drop(data);
                                if end.fetch_sub(1, Release) == 1 {
                                    main_thread.unpark();
                                }
                            })
                            .unwrap()
                            .into(),
                    )
                })
                .collect::<Box<_>>();

            (data, signal, threads)
        },
        |(data, signal, ..)| {
            let (begin, end) = &**signal;
            begin.store(true, Relaxed);

            while end.load(Acquire) > 0 {
                thread::park();
            }

            assert_eq!(Arc::get_mut(data).unwrap().len(), thread_count * DATA_LEN * APPEND_COUNT,);
        },
        BatchSize::SmallInput,
    );
}

fn bench(tests: &mut Criterion) {
    for &thread_count in THREAD_COUNT {
        let mut group = tests.benchmark_group(format!("Belt vs Mutex ({thread_count} threads)"));
        group.throughput(Throughput::Elements(
            thread_count as u64 * DATA_LEN as u64 * APPEND_COUNT as u64,
        ));

        for &chunk_len in CHUNK_LEN {
            group
                .bench_with_input(
                    BenchmarkId::new("belt", chunk_len),
                    black_box(&(thread_count, chunk_len)),
                    |bench, &(thread_count, chunk_len)| {
                        bench_vec_belt(bench, thread_count, black_box(APPEND_COUNT), chunk_len);
                    },
                )
                .bench_with_input(
                    BenchmarkId::new("mutex", chunk_len),
                    black_box(&(thread_count, chunk_len)),
                    |bench, &(thread_count, chunk_len)| {
                        bench_vec_mutex(bench, thread_count, black_box(APPEND_COUNT), chunk_len);
                    },
                );
        }

        group.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
