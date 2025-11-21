use alloy_primitives::U256;
use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use revive_integration::cases::Contract;
use revive_yul::{lexer::Lexer, parser::statement::object::Object};

/// The function under test parses the Yul `source_code`.
fn parse(source_code: &str) {
    let mut lexer = Lexer::new(source_code.to_owned());
    Object::parse(&mut lexer, None).expect("the Yul source should parse");
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(format!("{group_name} - parse"))
}

fn bench<F>(mut group: BenchmarkGroup<'_, WallTime>, contract: F)
where
    F: Fn() -> Contract,
{
    let source_code = contract().yul;

    group.sample_size(200);

    group.bench_function("parse", |b| {
        b.iter(|| parse(&source_code));
    });

    group.finish();
}

fn bench_baseline(c: &mut Criterion) {
    bench(group(c, "Baseline"), Contract::baseline);
}

fn bench_erc20(c: &mut Criterion) {
    bench(group(c, "ERC20"), Contract::erc20);
}

fn bench_sha1(c: &mut Criterion) {
    bench(group(c, "SHA1"), || Contract::sha1(vec![0xff].into()));
}

fn bench_storage(c: &mut Criterion) {
    bench(group(c, "Storage"), || {
        Contract::storage_transient(U256::from(0))
    });
}

fn bench_transfer(c: &mut Criterion) {
    bench(group(c, "Transfer"), || {
        Contract::transfer_self(U256::from(0))
    });
}

criterion_group!(
    name = benches_parse;
    config = Criterion::default();
    targets =
        bench_baseline,
        bench_erc20,
        bench_sha1,
        bench_storage,
        bench_transfer,
);
criterion_main!(benches_parse);
