//! The `resolc` compilation benchmarks.
//! The tests mimicking the commands run by these benchmarks exist in `src/tests/cli/bin.rs`.

use std::time::Duration;

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use resolc::{
    self,
    cli_utils::{absolute_path, execute_command, ResolcOptSettings, SolcOptSettings},
    SolcCompiler,
};

/// The function under test executes the `resolc` executable.
fn execute_resolc(arguments: &[&str], stdin_file_path: Option<&str>) {
    execute_command(resolc::DEFAULT_EXECUTABLE_NAME, arguments, stdin_file_path);
}

/// The function under test executes the `solc` executable.
fn execute_solc(arguments: &[&str], stdin_file_path: Option<&str>) {
    execute_command(
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
        arguments,
        stdin_file_path,
    );
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench(
    mut group: BenchmarkGroup<'_, WallTime>,
    resolc_arguments: &[&str],
    solc_arguments: &[&str],
    stdin_file_path: Option<&str>,
) {
    group.bench_function("resolc", |b| {
        b.iter(|| execute_resolc(resolc_arguments, stdin_file_path));
    });

    group.bench_function("solc", |b| {
        b.iter(|| execute_solc(solc_arguments, stdin_file_path));
    });

    group.finish();
}

fn bench_empty(c: &mut Criterion) {
    let mut group = group(c, "Empty");
    group
        .sample_size(100)
        .measurement_time(Duration::from_secs(8));
    let path = absolute_path("src/tests/data/solidity/contract.sol");
    let resolc_arguments = &[&path, "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--bin",
        "--via-ir",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_dependency(c: &mut Criterion) {
    let mut group = group(c, "Dependency");
    group
        .sample_size(50)
        .measurement_time(Duration::from_secs(9));
    let path = absolute_path("src/tests/data/solidity/dependency.sol");
    let resolc_arguments = &[&path, "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--bin",
        "--via-ir",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_large_div_rem(c: &mut Criterion) {
    let mut group = group(c, "LargeDivRem");
    group
        .sample_size(45)
        .measurement_time(Duration::from_secs(9));
    let path = absolute_path("src/tests/data/solidity/large_div_rem.sol");
    let resolc_arguments = &[&path, "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--bin",
        "--via-ir",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_memset(c: &mut Criterion) {
    let mut group = group(c, "Memset (`--yul`)");
    group
        .sample_size(100)
        .measurement_time(Duration::from_secs(7));
    let path = absolute_path("src/tests/data/yul/memset.yul");
    let resolc_arguments = &[&path, "--yul", "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--strict-assembly",
        "--bin",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_return(c: &mut Criterion) {
    let mut group = group(c, "Return (`--yul`)");
    group
        .sample_size(100)
        .measurement_time(Duration::from_secs(6));
    let path = absolute_path("src/tests/data/yul/return.yul");
    let resolc_arguments = &[&path, "--yul", "--bin", ResolcOptSettings::PERFORMANCE];
    let solc_arguments = &[
        &path,
        "--strict-assembly",
        "--bin",
        "--optimize",
        "--optimize-runs",
        SolcOptSettings::PERFORMANCE,
    ];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_standard_json_contracts(c: &mut Criterion) {
    let mut group = group(c, "Multiple Contracts (`--standard-json`)");
    group
        .sample_size(20)
        .measurement_time(Duration::from_secs(35));
    let path = absolute_path("src/tests/data/standard_json/solidity_contracts.json");
    let resolc_arguments = &["--standard-json"];
    let solc_arguments = &["--standard-json"];

    bench(group, resolc_arguments, solc_arguments, Some(&path));
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets =
        bench_empty,
        bench_dependency,
        bench_large_div_rem,
        bench_memset,
        bench_return,
        bench_standard_json_contracts,
);
criterion_main!(benches);
