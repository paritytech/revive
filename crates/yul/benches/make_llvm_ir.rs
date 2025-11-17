use alloy_primitives::*;
use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BatchSize, BenchmarkGroup, Criterion,
};
use inkwell::context::Context as InkwellContext;
use revive_integration::cases::Contract;
use revive_llvm_context::{
    initialize_llvm, Optimizer, OptimizerSettings, PolkaVMContext, PolkaVMTarget, PolkaVMWriteLLVM,
};
use revive_yul::{lexer::Lexer, parser::statement::object::Object as AstObject};

/// The function under test lowers the Yul `Object` into LLVM IR.
fn make_llvm_ir(mut ast: AstObject, mut llvm_context: PolkaVMContext) {
    ast.declare(&mut llvm_context)
        .expect("the AST should be valid");
    ast.into_llvm(&mut llvm_context)
        .expect("the AST should lower to LLVM IR");
}

fn parse(source_code: &str) -> AstObject {
    let mut lexer = Lexer::new(source_code.to_owned());
    AstObject::parse(&mut lexer, None).expect("expected a Yul AST Object")
}

fn create_llvm_context<'ctx>(
    llvm: &'ctx InkwellContext,
    optimizer_settings: &OptimizerSettings,
) -> PolkaVMContext<'ctx> {
    initialize_llvm(PolkaVMTarget::PVM, "resolc", Default::default());

    let module = llvm.create_module("module_bench");
    let optimizer = Optimizer::new(optimizer_settings.to_owned());

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

fn bench<F>(
    mut group: BenchmarkGroup<'_, WallTime>,
    contract: F,
    optimizer_settings: OptimizerSettings,
) where
    F: Fn() -> Contract,
{
    let llvm = InkwellContext::create();
    let ast = parse(&contract().yul);

    group.sample_size(10);

    group.bench_function("Revive", |b| {
        b.iter_batched(
            || (ast.clone(), create_llvm_context(&llvm, &optimizer_settings)),
            |(ast, llvm_context)| make_llvm_ir(ast, llvm_context),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_baseline(c: &mut Criterion) {
    bench(
        group(c, "Baseline - To LLVM IR"),
        Contract::baseline,
        OptimizerSettings::none(),
    );
}

fn bench_erc20(c: &mut Criterion) {
    bench(
        group(c, "ERC20 - To LLVM IR"),
        Contract::erc20,
        OptimizerSettings::none(),
    );
}

fn bench_sha1(c: &mut Criterion) {
    bench(
        group(c, "SHA1 - To LLVM IR"),
        || Contract::sha1(vec![0xff].into()),
        OptimizerSettings::none(),
    );
}

fn bench_storage(c: &mut Criterion) {
    bench(
        group(c, "Storage - To LLVM IR"),
        || Contract::storage_transient(U256::from(0)),
        OptimizerSettings::none(),
    );
}

fn bench_transfer(c: &mut Criterion) {
    bench(
        group(c, "Transfer - To LLVM IR"),
        || Contract::transfer_self(U256::from(0)),
        OptimizerSettings::none(),
    );
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets =
        bench_baseline,
        bench_erc20,
        bench_sha1,
        bench_storage,
        bench_transfer,
);
criterion_main!(benches);
