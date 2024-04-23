#[cfg(feature = "bench-extensive")]
use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, measurement::Measurement, BenchmarkGroup, BenchmarkId,
    Criterion,
};
#[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
use polkavm::BackendKind;

use revive_benchmarks::runtimes;
use revive_integration::cases::Contract;

fn bench<'a, P, L, I, M>(
    mut group: BenchmarkGroup<'a, M>,
    parameters: &[P],
    labels: &[L],
    contract: I,
) where
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
            let vm = runtimes::evm::prepare(contract.evm_runtime, contract.calldata);
            group.bench_with_input(BenchmarkId::new("EVM", l), p, move |b, _| {
                b.iter(|| {
                    runtimes::evm::execute(vm.clone());
                });
            });
        }

        #[cfg(feature = "bench-pvm-interpreter")]
        {
            let contract = contract(p.clone());
            let (state, mut instance, export) = runtimes::polkavm::prepare_pvm(
                &contract.pvm_runtime,
                &contract.calldata,
                BackendKind::Interpreter,
            );
            group.bench_with_input(BenchmarkId::new("PolkaVMInterpreter", l), p, |b, _| {
                b.iter(|| {
                    revive_integration::mock_runtime::call(state.clone(), &mut instance, export);
                });
            });
        }

        #[cfg(feature = "bench-pvm-compiler")]
        {
            let contract = contract(p.clone());
            let (state, mut instance, export) = runtimes::polkavm::prepare_pvm(
                &contract.pvm_runtime,
                &contract.calldata,
                BackendKind::Compiler,
            );
            group.bench_with_input(BenchmarkId::new("PolkaVM", l), p, |b, _| {
                b.iter(|| {
                    revive_integration::mock_runtime::call(state.clone(), &mut instance, export);
                });
            });
        }
    }

    group.finish();
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
    let mut group = c.benchmark_group("OddProduct");
    group.sample_size(20);
    #[cfg(feauture = "bench-extensive")]
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(60));

    #[cfg(feauture = "bench-extensive")]
    let parameters = &[2_000_000i32, 4_000_000, 8_000_000, 120_000_000];
    #[cfg(not(feauture = "bench-extensive"))]
    let parameters = &[100_000];

    bench(group, parameters, parameters, |p| Contract::odd_product(p));
}

fn bench_triangle_number(c: &mut Criterion) {
    let mut group = c.benchmark_group("TriangleNumber");
    group.sample_size(20);
    #[cfg(feauture = "bench-extensive")]
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(60));

    #[cfg(feauture = "bench-extensive")]
    let parameters = &[3_000_000i64, 6_000_000, 12_000_000, 180_000_000];
    #[cfg(not(feauture = "bench-extensive"))]
    let parameters = &[100_000];

    bench(group, parameters, parameters, |p| {
        Contract::triangle_number(p)
    });
}

fn bench_fibonacci_recurisve(c: &mut Criterion) {
    let parameters = &[8, 12, 16, 18, 20];

    bench(
        c.benchmark_group("FibonacciRecursive"),
        parameters,
        parameters,
        |p| Contract::fib_recursive(p),
    );
}

fn bench_fibonacci_iterative(c: &mut Criterion) {
    let parameters = &[32, 64, 128, 256];

    bench(
        c.benchmark_group("FibonacciIterative"),
        parameters,
        parameters,
        |p| Contract::fib_iterative(p),
    );
}

fn bench_fibonacci_binet(c: &mut Criterion) {
    let parameters = &[32, 64, 128, 256];

    bench(
        c.benchmark_group("FibonacciBinet"),
        parameters,
        parameters,
        |p| Contract::fib_binet(p),
    );
}

fn bench_sha1(c: &mut Criterion) {
    #[cfg(not(feauture = "bench-extensive"))]
    let parameters = &[vec![0xff], vec![0xff; 64], vec![0xff; 256], vec![0xff; 512]];
    #[cfg(feauture = "bench-extensive")]
    let parameters = &[vec![0xff; 512], vec![0xff, 1024], vec![0xff, 2048]];
    let labels = parameters.iter().map(|p| p.len()).collect::<Vec<_>>();

    bench(c.benchmark_group("SHA1"), parameters, &labels, |p| {
        Contract::sha1(p)
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
