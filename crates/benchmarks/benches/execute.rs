#[cfg(feature = "bench-extensive")]
use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, measurement::Measurement, BenchmarkGroup, BenchmarkId,
    Criterion,
};
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
            let (state, mut instance, export) = revive_benchmarks::prepare_pvm(
                &contract.pvm_runtime,
                &contract.calldata,
                polkavm::BackendKind::Interpreter,
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
            let (state, mut instance, export) = revive_benchmarks::prepare_pvm(
                &contract.pvm_runtime,
                &contract.calldata,
                polkavm::BackendKind::Compiler,
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

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    #[cfg(feature = "bench-extensive")]
    {
        let mut group = c.benchmark_group(group_name);
        group
            .sample_size(10)
            .measurement_time(Duration::from_secs(60));
        group
    }

    #[cfg(not(feature = "bench-extensive"))]
    return c.benchmark_group(group_name);
}

fn bench_baseline(c: &mut Criterion) {
    let group = group(c, "Baseline");
    let parameters = &[0u8];

    bench(group, parameters, parameters, |_| Contract::baseline());
}

fn bench_odd_product(c: &mut Criterion) {
    let group = group(c, "OddPorduct");
    #[cfg(feature = "bench-extensive")]
    let parameters = &[300000, 1200000, 12000000, 180000000, 720000000];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[10_000, 100_000];

    bench(group, parameters, parameters, Contract::odd_product);
}

fn bench_triangle_number(c: &mut Criterion) {
    let group = group(c, "TriangleNumber");
    #[cfg(feature = "bench-extensive")]
    let parameters = &[360000, 1440000, 14400000, 216000000, 864000000];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[10_000, 100_000];

    bench(group, parameters, parameters, Contract::triangle_number);
}

fn bench_fibonacci_recurisve(c: &mut Criterion) {
    let group = group(c, "FibonacciRecursive");
    #[cfg(feature = "bench-extensive")]
    let parameters = &[24, 27, 31, 36, 39];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[12, 16, 20];

    bench(group, parameters, parameters, Contract::fib_recursive);
}

fn bench_fibonacci_iterative(c: &mut Criterion) {
    let group = group(c, "FibonacciIterative");
    #[cfg(feature = "bench-extensive")]
    let parameters = &[256, 162500, 650000, 6500000, 100000000, 400000000];
    #[cfg(not(feature = "bench-extensive"))]
    let parameters = &[64, 128, 256];

    bench(group, parameters, parameters, Contract::fib_iterative);
}

fn bench_fibonacci_binet(c: &mut Criterion) {
    let group = group(c, "FibonacciBinet");
    let parameters = &[64, 128, 256];

    bench(group, parameters, parameters, Contract::fib_binet);
}

fn bench_sha1(c: &mut Criterion) {
    let group = group(c, "SHA1");
    let parameters = &[vec![0xff], vec![0xff; 64], vec![0xff; 512]];
    let labels = parameters.iter().map(|p| p.len()).collect::<Vec<_>>();

    bench(group, parameters, &labels, Contract::sha1);
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
