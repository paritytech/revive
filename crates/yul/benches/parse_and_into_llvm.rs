#![cfg(feature = "bench-yul")]

use std::time::{Duration, Instant};

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use inkwell::context::Context as InkwellContext;
use revive_llvm_context::{
    initialize_llvm, Optimizer, OptimizerSettings, PolkaVMContext, PolkaVMTarget, PolkaVMWriteLLVM,
};
use revive_yul::{lexer::Lexer, parser::statement::object::Object as AstObject};

fn measure(
    source_code: &str,
    llvm: &InkwellContext,
    optimizer_settings: &OptimizerSettings,
    iters: u64,
) -> Duration {
    let mut total_time = Duration::default();

    for i in 0..iters {
        let llvm_module_name = format!("module_{}", i);
        let mut llvm_context = create_llvm_context(&llvm, &llvm_module_name, optimizer_settings);

        let start = Instant::now();
        yul_to_llvm_ir(source_code, &mut llvm_context);
        total_time += start.elapsed();
    }

    total_time
}

#[inline(always)]
fn yul_to_llvm_ir(source_code: &str, llvm_context: &mut PolkaVMContext) {
    let mut ast = parse_yul(source_code);
    ast.declare(llvm_context).unwrap();
    ast.into_llvm(llvm_context).unwrap();
}

#[inline(always)]
fn parse_yul(source_code: &str) -> AstObject {
    let mut lexer = Lexer::new(source_code.to_owned());
    AstObject::parse(&mut lexer, None).unwrap()
}

fn create_llvm_context<'ctx>(
    llvm: &'ctx InkwellContext,
    module_name: &str,
    optimizer_settings: &OptimizerSettings,
) -> PolkaVMContext<'ctx> {
    initialize_llvm(PolkaVMTarget::PVM, "resolc", Default::default());

    let module = llvm.create_module(module_name);
    let optimizer = Optimizer::new(optimizer_settings.to_owned());

    PolkaVMContext::new(
        llvm,
        module,
        optimizer,
        Default::default(),
        Default::default(),
    )
}

fn bench(
    mut group: BenchmarkGroup<'_, WallTime>,
    source_code: &str,
    optimizer_settings: OptimizerSettings,
) {
    let llvm = InkwellContext::create();

    group.sample_size(10);

    group.bench_function("Yul -> LLVM IR", |b| {
        b.iter_custom(|iters| measure(source_code, &llvm, &optimizer_settings, iters));
    });

    group.finish();
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench_memset(c: &mut Criterion) {
    let group = group(c, "Memset");
    let source_code = include_str!("../../resolc/src/tests/data/yul/memset.yul");

    bench(group, source_code, OptimizerSettings::none());
}

criterion_group!(
    name = compile;
    config = Criterion::default();
    targets = bench_memset
);
criterion_main!(compile);
