#![cfg(any(feature = "bench-pvm-interpreter", feature = "bench-evm"))]

use alloy_primitives::U256;
use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, BenchmarkId, Criterion,
};
use revive_integration::cases::Contract;

fn bench<P, L, I>(
    mut group: BenchmarkGroup<'_, WallTime>,
    parameters: &[P],
    labels: &[L],
    contract: I,
) where
    P: Clone,
    L: std::fmt::Display,
    I: Fn(P) -> Contract,
{
    assert_eq!(parameters.len(), labels.len());

    group.sample_size(10);

    for (p, l) in parameters.iter().zip(labels.iter()) {
        let contract = contract(p.clone());

        #[cfg(feature = "bench-evm")]
        group.bench_with_input(BenchmarkId::new("EVM", l), p, |b, _| {
            let code = &contract.evm_runtime;
            let input = &contract.calldata;
            b.iter_custom(|iters| revive_benchmarks::measure_evm(code, input, iters));
        });

        #[cfg(feature = "bench-pvm-interpreter")]
        group.bench_with_input(BenchmarkId::new("PVMInterpreter", l), p, |b, _| {
            let specs = revive_benchmarks::create_specs(&contract);
            b.iter_custom(|iters| revive_benchmarks::measure_pvm(&specs, iters));
        });
    }

    group.finish();
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    return c.benchmark_group(group_name);
}

fn bench_baseline(c: &mut Criterion) {
    let group = group(c, "Baseline");
    let parameters = &[0u8];

    bench(group, parameters, parameters, |_| Contract::baseline());
}

fn bench_odd_product(c: &mut Criterion) {
    let group = group(c, "OddPorduct");
    let parameters = &[10_000, 100_000, 300000];

    bench(group, parameters, parameters, Contract::odd_product);
}

fn bench_triangle_number(c: &mut Criterion) {
    let group = group(c, "TriangleNumber");
    let parameters = &[10_000, 100_000, 360000];

    bench(group, parameters, parameters, Contract::triangle_number);
}

fn bench_fibonacci_recurisve(c: &mut Criterion) {
    let group = group(c, "FibonacciRecursive");
    let parameters = [12, 16, 20, 24]
        .iter()
        .map(|p| U256::from(*p))
        .collect::<Vec<_>>();

    bench(group, &parameters, &parameters, Contract::fib_recursive);
}

fn bench_fibonacci_iterative(c: &mut Criterion) {
    let group = group(c, "FibonacciIterative");
    let parameters = [64, 128, 256]
        .iter()
        .map(|p| U256::from(*p))
        .collect::<Vec<_>>();

    bench(group, &parameters, &parameters, Contract::fib_iterative);
}

fn bench_fibonacci_binet(c: &mut Criterion) {
    let group = group(c, "FibonacciBinet");
    let parameters = [64, 128, 256]
        .iter()
        .map(|p| U256::from(*p))
        .collect::<Vec<_>>();

    bench(group, &parameters, &parameters, Contract::fib_binet);
}

fn bench_sha1(c: &mut Criterion) {
    let group = group(c, "SHA1");
    let parameters = &[vec![0xff], vec![0xff; 64], vec![0xff; 512]];
    let labels = parameters.iter().map(|p| p.len()).collect::<Vec<_>>();

    bench(group, parameters, &labels, |input| {
        Contract::sha1(input.into())
    });
}

criterion_group!(
    name = execute;
    config = Criterion::default();
    targets = bench_baseline,
    bench_odd_product,
    bench_triangle_number,
    bench_fibonacci_recurisve,
    bench_fibonacci_iterative,
    bench_fibonacci_binet,
    bench_sha1
);
criterion_main!(execute);
