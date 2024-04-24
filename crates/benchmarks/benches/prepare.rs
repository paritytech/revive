use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use polkavm::BackendKind;

use revive_benchmarks::runtimes;
use revive_integration::cases::Contract;

fn bench(
    c: &mut Criterion,
    group_name: &str,
    #[cfg(feature = "bench-evm")] evm_runtime: Vec<u8>,
    #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))] pvm_runtime: Vec<u8>,
) {
    let mut group = c.benchmark_group(group_name);
    let code_size = 0;

    #[cfg(feature = "bench-evm")]
    group.bench_with_input(
        BenchmarkId::new("Evm", code_size),
        &evm_runtime,
        |b, code| b.iter(|| revive_differential::prepare(code.clone(), Vec::new())),
    );

    #[cfg(feature = "bench-pvm-interpreter")]
    {
        let engine = runtimes::polkavm::instantiate_engine(BackendKind::Interpreter);
        group.bench_with_input(
            BenchmarkId::new("PVMInterpreterCompile", code_size),
            &(&pvm_runtime, engine),
            |b, (code, engine)| {
                b.iter(|| {
                    revive_integration::mock_runtime::recompile_code(code, engine);
                });
            },
        );
    }

    #[cfg(feature = "bench-pvm-interpreter")]
    {
        let engine = runtimes::polkavm::instantiate_engine(BackendKind::Interpreter);
        let module = revive_integration::mock_runtime::recompile_code(&pvm_runtime, &engine);
        group.bench_with_input(
            BenchmarkId::new("PVMInterpreterInstantiate", code_size),
            &(module, engine),
            |b, (module, engine)| {
                b.iter(|| {
                    revive_integration::mock_runtime::instantiate_module(module, engine);
                });
            },
        );
    }

    #[cfg(feature = "bench-pvm")]
    {
        let engine = runtimes::polkavm::instantiate_engine(BackendKind::Compiler);
        group.bench_with_input(
            BenchmarkId::new("PVMCompile", code_size),
            &(&pvm_runtime, engine),
            |b, (code, engine)| {
                b.iter(|| {
                    revive_integration::mock_runtime::recompile_code(code, engine);
                });
            },
        );
    }

    #[cfg(feature = "bench-pvm")]
    {
        let engine = runtimes::polkavm::instantiate_engine(BackendKind::Compiler);
        let module = revive_integration::mock_runtime::recompile_code(&pvm_runtime, &engine);
        group.bench_with_input(
            BenchmarkId::new("PVMInstantiate", code_size),
            &(module, engine),
            |b, (module, engine)| {
                b.iter(|| {
                    revive_integration::mock_runtime::instantiate_module(module, engine);
                });
            },
        );
    }

    group.finish();
}

fn bench_baseline(c: &mut Criterion) {
    bench(
        c,
        "PrepareBaseline",
        #[cfg(feature = "bench-evm")]
        Contract::baseline().evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::baseline().pvm_runtime,
    );
}

fn bench_odd_product(c: &mut Criterion) {
    bench(
        c,
        "PrepareOddProduct",
        #[cfg(feature = "bench-evm")]
        Contract::odd_product(0).evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::baseline().pvm_runtime,
    );
}

fn bench_triangle_number(c: &mut Criterion) {
    bench(
        c,
        "PrepareTriangleNumber",
        #[cfg(feature = "bench-evm")]
        Contract::triangle_number(0).evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::triangle_number(0).pvm_runtime,
    );
}

fn bench_fibonacci_recursive(c: &mut Criterion) {
    bench(
        c,
        "PrepareFibonacciRecursive",
        #[cfg(feature = "bench-evm")]
        Contract::fib_recursive(0).evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::fib_recursive(0).pvm_runtime,
    );
}

fn bench_fibonacci_iterative(c: &mut Criterion) {
    bench(
        c,
        "PrepareFibonacciIterative",
        #[cfg(feature = "bench-evm")]
        Contract::fib_iterative(0).evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::fib_iterative(0).pvm_runtime,
    );
}

fn bench_fibonacci_binet(c: &mut Criterion) {
    bench(
        c,
        "PrepareFibonacciBinet",
        #[cfg(feature = "bench-evm")]
        Contract::fib_binet(0).evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::fib_binet(0).pvm_runtime,
    );
}

fn bench_sha1(c: &mut Criterion) {
    bench(
        c,
        "PrepareSHA1",
        #[cfg(feature = "bench-evm")]
        Contract::sha1(Default::default()).evm_runtime,
        #[cfg(any(feature = "bench-pvm-interpreter", feature = "bench-pvm"))]
        Contract::sha1(Default::default()).pvm_runtime,
    );
}

criterion_group!(
    name = prepare;
    config = Criterion::default();
    targets = bench_baseline,
    bench_odd_product,
    bench_triangle_number,
    bench_fibonacci_recursive,
    bench_fibonacci_iterative,
    bench_fibonacci_binet,
    bench_sha1
);
criterion_main!(prepare);
