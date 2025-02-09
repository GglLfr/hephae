use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Bencher, BenchmarkId, Criterion};
use hephae_utils::{sync::*, vec_belt::VecBelt};

const INITIAL_LEN: usize = 32768;

const DATA_LEN: usize = 64;
const APPEND_COUNT: &[usize] = &[4, 16, 64];
const THREAD_COUNT: &[usize] = &[4, 16, 64];

struct Join(Option<thread::JoinHandle<()>>);
impl Drop for Join {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.join().unwrap();
        }
    }
}

fn bench_vec_mutex(bench: &mut Bencher, thread_count: usize, append_count: usize, data_len: usize) {
    let main_thread = thread::current();
    bench.iter_batched_ref(
        || {
            let data = Arc::new(Mutex::new(Vec::<usize>::with_capacity(INITIAL_LEN)));
            let signal = Arc::new((AtomicBool::new(false), AtomicUsize::new(thread_count)));
            let proceed = Arc::new(AtomicUsize::new(thread_count));

            let threads = (0..thread_count)
                .map(|i| {
                    let input = (0..data_len).map(|num| num + i * data_len).collect::<Box<_>>();
                    let main_thread = main_thread.clone();
                    let data = data.clone();
                    let signal = signal.clone();
                    let proceed = proceed.clone();

                    Join(
                        thread::Builder::new()
                            .stack_size(1024)
                            .spawn(move || {
                                if proceed.fetch_sub(1, Relaxed) == 1 {
                                    main_thread.unpark();
                                }

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

            while proceed.load(Relaxed) > 0 {
                thread::park();
            }

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
                thread_count * data_len * append_count
            );
        },
        BatchSize::SmallInput,
    );
}

fn bench_vec_belt(bench: &mut Bencher, thread_count: usize, append_count: usize, data_len: usize) {
    let main_thread = thread::current();
    bench.iter_batched_ref(
        || {
            let data = Arc::new(VecBelt::<usize>::new(INITIAL_LEN));
            let signal = Arc::new((AtomicBool::new(false), AtomicUsize::new(thread_count)));
            let proceed = Arc::new(AtomicUsize::new(thread_count));

            let threads = (0..thread_count)
                .map(|i| {
                    let input = (0..data_len).map(|num| num + i * data_len).collect::<Box<_>>();
                    let main_thread = main_thread.clone();
                    let data = data.clone();
                    let signal = signal.clone();
                    let proceed = proceed.clone();

                    Join(
                        thread::Builder::new()
                            .stack_size(1024)
                            .spawn(move || {
                                if proceed.fetch_sub(1, Relaxed) == 1 {
                                    main_thread.unpark();
                                }

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

            while proceed.load(Relaxed) > 0 {
                thread::park();
            }

            (data, signal, threads)
        },
        |(data, signal, ..)| {
            let (begin, end) = &**signal;
            begin.store(true, Relaxed);

            while end.load(Acquire) > 0 {
                thread::park();
            }

            assert_eq!(data.len(), thread_count * data_len * append_count);
        },
        BatchSize::SmallInput,
    );
}

fn bench(tests: &mut Criterion) {
    for &thread_count in THREAD_COUNT {
        let mut group = tests.benchmark_group(format!("Belt vs Mutex ({thread_count} threads)"));
        for &append_count in APPEND_COUNT {
            group
                .bench_with_input(
                    BenchmarkId::new("belt", append_count),
                    black_box(&(thread_count, append_count)),
                    |bench, &(thread_count, append_count)| {
                        bench_vec_belt(bench, thread_count, append_count, black_box(DATA_LEN));
                    },
                )
                .bench_with_input(
                    BenchmarkId::new("mutex", append_count),
                    black_box(&(thread_count, append_count)),
                    |bench, &(thread_count, append_count)| {
                        bench_vec_mutex(bench, thread_count, append_count, black_box(DATA_LEN));
                    },
                );
        }

        group.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
