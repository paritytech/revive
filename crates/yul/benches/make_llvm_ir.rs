#![cfg(feature = "bench-llvm-ir")]

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BatchSize, BenchmarkGroup, Criterion,
};
use inkwell::context::Context as InkwellContext;
use revive_llvm_context::{
    initialize_llvm, Optimizer, OptimizerSettings, PolkaVMContext, PolkaVMTarget, PolkaVMWriteLLVM,
};
use revive_yul::{lexer::Lexer, parser::statement::object::Object as AstObject};

/// Function under test - Lower the Yul AST Object into LLVM IR.
fn make_llvm_ir(mut ast: AstObject, mut llvm_context: PolkaVMContext) {
    ast.declare(&mut llvm_context)
        .expect("expected LLVM IR generation");
    ast.into_llvm(&mut llvm_context)
        .expect("expected LLVM IR generation");
}

fn per_iteration_setup<'ctx>(
    source_code: &str,
    llvm: &'ctx InkwellContext,
    optimizer_settings: OptimizerSettings,
) -> (AstObject, PolkaVMContext<'ctx>) {
    let ast = parse(source_code);
    let llvm_context = create_llvm_context(llvm, "module_bench", optimizer_settings);

    (ast, llvm_context)
}

fn parse(source_code: &str) -> AstObject {
    let mut lexer = Lexer::new(source_code.to_owned());
    AstObject::parse(&mut lexer, None).expect("expected a Yul AST Object")
}

fn create_llvm_context<'ctx>(
    llvm: &'ctx InkwellContext,
    module_name: &str,
    optimizer_settings: OptimizerSettings,
) -> PolkaVMContext<'ctx> {
    initialize_llvm(PolkaVMTarget::PVM, "resolc", Default::default());

    let module = llvm.create_module(module_name);
    let optimizer = Optimizer::new(optimizer_settings);

    PolkaVMContext::new(
        llvm,
        module,
        optimizer,
        Default::default(),
        Default::default(),
    )
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench(
    mut group: BenchmarkGroup<'_, WallTime>,
    source_code: &str,
    optimizer_settings: OptimizerSettings,
) {
    let llvm = InkwellContext::create();

    group.sample_size(10);

    group.bench_function("Revive", |b| {
        b.iter_batched(
            || per_iteration_setup(source_code, &llvm, optimizer_settings.clone()),
            |(ast, llvm_context)| make_llvm_ir(ast, llvm_context),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_memset(c: &mut Criterion) {
    let group = group(c, "Memset - To LLVM IR");
    let source_code = include_str!("../../resolc/src/tests/data/yul/memset.yul");

    bench(group, source_code, OptimizerSettings::none());
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = bench_memset
);
criterion_main!(benches);
