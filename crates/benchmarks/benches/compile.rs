#![cfg(feature = "bench-resolc")]

use std::time::Duration;

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use revive_benchmarks::contracts::Contract;

fn bench(mut group: BenchmarkGroup<'_, WallTime>, compiler_arguments: &[&str]) {
    group.measurement_time(Duration::from_secs(50));
    group.sample_size(10);

    #[cfg(feature = "bench-resolc")]
    group.bench_function("Resolc", |b| {
        b.iter_custom(|iters| revive_benchmarks::measure_resolc(iters, compiler_arguments));
    });

    group.finish();
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench_store_uint256(c: &mut Criterion) {
    let contract = Contract::store_uint256_n_times(1000);
    let group = group(c, &contract.name);
    let compiler_arguments = &[&contract.path, "-O0"];

    bench(group, compiler_arguments);
}

criterion_group!(
    name = compile;
    config = Criterion::default();
    targets = bench_store_uint256
);
criterion_main!(compile);
