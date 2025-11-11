use std::{
    fs::File,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::Duration,
};

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BatchSize, BenchmarkGroup, Criterion,
};
use resolc::{self, SolcCompiler};

/// The function under test executes the `resolc` executable.
fn execute_resolc(arguments: &[&str], stdin_config: Stdio) {
    execute_command(resolc::DEFAULT_EXECUTABLE_NAME, arguments, stdin_config);
}

/// The function under test executes the `solc` executable.
fn execute_solc(arguments: &[&str], stdin_config: Stdio) {
    execute_command(
        SolcCompiler::DEFAULT_EXECUTABLE_NAME,
        arguments,
        stdin_config,
    );
}

#[inline(always)]
fn execute_command(command: &str, arguments: &[&str], stdin_config: Stdio) {
    let result = Command::new(command)
        .args(arguments)
        .stdin(stdin_config)
        .output()
        .expect("expected command output");

    assert!(
        result.status.success(),
        "command failed: {}",
        get_stderr(&result)
    );
}

fn get_stderr(result: &Output) -> String {
    String::from_utf8_lossy(&result.stderr).to_string()
}

fn get_stdin_config(stdin_file_path: Option<&str>) -> Stdio {
    match stdin_file_path {
        Some(path) => Stdio::from(File::open(path).unwrap()),
        None => Stdio::null(),
    }
}

/// Gets the absolute path of a file. The `relative_path` must
/// be relative to the `resolc` crate.
/// Panics if the path does not exist or is not an accessible file.
fn absolute_path(relative_path: &str) -> String {
    let absolute_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    if !absolute_path.is_file() {
        panic!("expected a file at `{}`", absolute_path.display());
    }

    absolute_path.to_string_lossy().into_owned()
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
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    group.bench_function("resolc", |b| {
        b.iter_batched(
            || get_stdin_config(stdin_file_path),
            |stdin_config| execute_resolc(resolc_arguments, stdin_config),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("solc", |b| {
        b.iter_batched(
            || get_stdin_config(stdin_file_path),
            |stdin_config| execute_solc(solc_arguments, stdin_config),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_empty(c: &mut Criterion) {
    let group = group(c, "Empty");
    let path = absolute_path("src/tests/data/solidity/contract.sol");
    let resolc_arguments = &[&path, "-O3"];
    let solc_arguments = &[&path, "--optimize"];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_dependency(c: &mut Criterion) {
    let group = group(c, "Dependency");
    let path = absolute_path("src/tests/data/solidity/dependency.sol");
    let resolc_arguments = &[&path, "-O3"];
    let solc_arguments = &[&path, "--optimize"];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_large_div_rem(c: &mut Criterion) {
    let group = group(c, "LargeDivRem");
    let path = absolute_path("src/tests/data/solidity/large_div_rem.sol");
    let resolc_arguments = &[&path, "-O3"];
    let solc_arguments = &[&path, "--optimize"];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_memset(c: &mut Criterion) {
    let group = group(c, "Memset (`--yul`)");
    let path = absolute_path("src/tests/data/yul/memset.yul");
    let resolc_arguments = &[&path, "--yul", "-O3"];
    let solc_arguments = &[&path, "--strict-assembly", "--optimize"];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_return(c: &mut Criterion) {
    let group = group(c, "Return (`--yul`)");
    let path = absolute_path("src/tests/data/yul/return.yul");
    let resolc_arguments = &[&path, "--yul", "-O3"];
    let solc_arguments = &[&path, "--strict-assembly", "--optimize"];

    bench(group, resolc_arguments, solc_arguments, None);
}

fn bench_standard_json_contracts(c: &mut Criterion) {
    let group = group(c, "Multiple Contracts (`--standard-json`)");
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
