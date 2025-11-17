use alloy_primitives::*;
use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BatchSize, BenchmarkGroup, Criterion,
};
use revive_integration::cases::Contract;
use revive_yul::{lexer::Lexer, parser::statement::object::Object as AstObject};

/// The function under test parses the Yul `source_code`.
/// Consumes the source code String in order to not time the cloning.
fn parse(source_code: String) {
    let mut lexer = Lexer::new(source_code);
    AstObject::parse(&mut lexer, None).expect("expected a Yul AST Object");
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench<F>(mut group: BenchmarkGroup<'_, WallTime>, contract: F)
where
    F: Fn() -> Contract,
{
    let source_code = contract().yul;

    group.sample_size(200);

    group.bench_function("parse", |b| {
        b.iter_batched(|| source_code.clone(), parse, BatchSize::SmallInput);
    });

    group.finish();
}

fn bench_baseline(c: &mut Criterion) {
    bench(group(c, "Baseline - Parse"), Contract::baseline);
}

fn bench_erc20(c: &mut Criterion) {
    bench(group(c, "ERC20 - Parse"), Contract::erc20);
}

fn bench_sha1(c: &mut Criterion) {
    bench(group(c, "SHA1 - Parse"), || {
        Contract::sha1(vec![0xff].into())
    });
}

fn bench_storage(c: &mut Criterion) {
    bench(group(c, "Storage - Parse"), || {
        Contract::storage_transient(U256::from(0))
    });
}

fn bench_transfer(c: &mut Criterion) {
    bench(group(c, "Transfer - Parse"), || {
        Contract::transfer_self(U256::from(0))
    });
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
