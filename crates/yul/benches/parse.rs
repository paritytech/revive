#![cfg(feature = "bench-parse")]

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use revive_yul::{lexer::Lexer, parser::statement::object::Object as AstObject};

/// Function under test - Parse Yul source code.
fn parse(source_code: &str) {
    let mut lexer = Lexer::new(source_code.to_owned());
    AstObject::parse(&mut lexer, None).expect("expected a Yul AST Object");
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench(mut group: BenchmarkGroup<'_, WallTime>, source_code: &str) {
    group.sample_size(10);

    group.bench_function("Revive", |b| {
        b.iter(|| parse(source_code));
    });

    group.finish();
}

fn bench_memset(c: &mut Criterion) {
    let group = group(c, "Memset - Parse");
    let source_code = include_str!("../../resolc/src/tests/data/yul/memset.yul");

    bench(group, source_code);
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = bench_memset
);
criterion_main!(benches);
