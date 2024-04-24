#[cfg(feature = "bench-extensive")]
use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, measurement::Measurement, BenchmarkGroup, BenchmarkId,
    Criterion,
};
#[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
use polkavm::BackendKind;

use revive_benchmarks::prepare_pvm;
use revive_integration::cases::Contract;

fn bench<P, L, I, M>(mut group: BenchmarkGroup<'_, M>, parameters: &[P], labels: &[L], contract: I)
where
    P: Clone,
    L: std::fmt::Display,
    I: Fn(P) -> Contract,
    M: Measurement,
{
    assert_eq!(parameters.len(), labels.len());

    for (p, l) in parameters.iter().zip(labels.iter()) {
        #[cfg(feature = "bench-evm")]
        {
            let contract = contract(p.clone());
            let vm = revive_differential::prepare(contract.evm_runtime, contract.calldata);
            group.bench_with_input(BenchmarkId::new("EVM", l), p, move |b, _| {
                b.iter(|| {
                    revive_differential::execute(vm.clone());
                });
            });
        }

        #[cfg(feature = "bench-pvm-interpreter")]
        {
            let contract = contract(p.clone());
            let (state, mut instance, export) = prepare_pvm(
                &contract.pvm_runtime,
                &contract.calldata,
                BackendKind::Interpreter,
            );
            group.bench_with_input(BenchmarkId::new("PVMInterpreter", l), p, |b, _| {
                b.iter(|| {
                    revive_integration::mock_runtime::call(state.clone(), &mut instance, export);
                });
            });
        }

        #[cfg(feature = "bench-pvm")]
        {
            let contract = contract(p.clone());
            let (state, mut instance, export) = prepare_pvm(
                &contract.pvm_runtime,
                &contract.calldata,
                BackendKind::Compiler,
            );
            group.bench_with_input(BenchmarkId::new("PVM", l), p, |b, _| {
                b.iter(|| {
                    revive_integration::mock_runtime::call(state.clone(), &mut instance, export);
                });
            });
        }
    }

    group.finish();
}

#[cfg(feature = "bench-extensive")]
fn group_extensive<'error, M>(
    c: &'error mut Criterion<M>,
    group_name: &str,
) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    let mut group = c.benchmark_group(group_name);
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(60));
    group
}

fn bench_baseline(c: &mut Criterion) {
    let parameters = &[0u8];

    bench(
        c.benchmark_group("Baseline"),
        parameters,
        parameters,
        |_| Contract::baseline(),
    );
}

fn bench_odd_product(c: &mut Criterion) {
    #[cfg(feature = "bench-extensive")]
    let group = group_extensive(c, "OddProduct");
    #[cfg(not(feature = "bench-extensive"))]
    let group = c.benchmark_group("OddProduct");

    #[cfg(feature = "bench-extensive")]
    let parameters = &[2_000_000i32, 4_000_000, 8_000_000, 120_000_000];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[10_000, 100_000];

    bench(group, parameters, parameters, Contract::odd_product);
}

fn bench_triangle_number(c: &mut Criterion) {
    #[cfg(feature = "bench-extensive")]
    let group = group_extensive(c, "TriangleNumber");
    #[cfg(not(feature = "bench-extensive"))]
    let group = c.benchmark_group("TriangleNumber");

    #[cfg(feature = "bench-extensive")]
    let parameters = &[3_000_000i64, 6_000_000, 12_000_000, 180_000_000];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[10_000, 100_000];

    bench(group, parameters, parameters, Contract::triangle_number);
}

fn bench_fibonacci_recurisve(c: &mut Criterion) {
    #[cfg(not(feature = "bench-extensive"))]
    let group = c.benchmark_group("FibonacciRecursive");
    #[cfg(feature = "bench-extensive")]
    let group = group_extensive(c, "FibonacciRecursive");

    #[cfg(feature = "bench-extensive")]
    let parameters = &[26, 30, 34, 38];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[12, 16, 20];

    bench(group, parameters, parameters, Contract::fib_recursive);
}

fn bench_fibonacci_iterative(c: &mut Criterion) {
    #[cfg(not(feature = "bench-extensive"))]
    let group = c.benchmark_group("FibonacciIterative");
    #[cfg(feature = "bench-extensive")]
    let group = group_extensive(c, "FibonacciIterative");

    #[cfg(feature = "bench-extensive")]
    let parameters = &[256, 100000, 1000000, 100000000];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[64, 128, 256];

    bench(group, parameters, parameters, Contract::fib_iterative);
}

fn bench_fibonacci_binet(c: &mut Criterion) {
    let parameters = &[64, 128, 256];

    bench(
        c.benchmark_group("FibonacciBinet"),
        parameters,
        parameters,
        Contract::fib_binet,
    );
}

fn bench_sha1(c: &mut Criterion) {
    let parameters = &[vec![0xff], vec![0xff; 64], vec![0xff; 512]];
    let labels = parameters.iter().map(|p| p.len()).collect::<Vec<_>>();

    bench(
        c.benchmark_group("SHA1"),
        parameters,
        &labels,
        Contract::sha1,
    );
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
