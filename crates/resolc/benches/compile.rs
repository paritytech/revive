use std::{
    path::PathBuf,
    process::{Command, Output},
    time::Duration,
};

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};

/// The function under test executes the `resolc` executable.
fn execute_resolc(arguments: &[&str]) {
    execute_command("resolc", arguments);
}

/// The function under test executes the `solc` executable.
fn execute_solc(arguments: &[&str]) {
    execute_command("solc", arguments);
}

#[inline(always)]
fn execute_command(command: &str, arguments: &[&str]) {
    let result = Command::new(command)
        .args(arguments)
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

/// Gets the absolute path of a file. The `relative_path` must
/// be relative to this file.
/// Panics if the path does not exist or is not an accessible file.
fn get_absolute_path(relative_path: &str) -> String {
    let this_file = PathBuf::from(file!());
    let this_directory = this_file.parent().expect("expected a parent directory");
    let absolute_path = this_directory.join(relative_path);

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
) {
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    group.bench_function("resolc", |b| {
        b.iter(|| execute_resolc(resolc_arguments));
    });

    group.bench_function("solc", |b| {
        b.iter(|| execute_solc(solc_arguments));
    });

    group.finish();
}

fn bench_empty(c: &mut Criterion) {
    let group = group(c, "Empty");
    let path = get_absolute_path("../src/tests/data/solidity/contract.sol");
    let resolc_arguments = &[&path, "-O3"];
    let solc_arguments = &[&path, "--optimize"];

    bench(group, resolc_arguments, solc_arguments);
}

fn bench_dependency(c: &mut Criterion) {
    let group = group(c, "Dependency");
    let path = get_absolute_path("../src/tests/data/solidity/dependency.sol");
    let resolc_arguments = &[&path, "-O3"];
    let solc_arguments = &[&path, "--optimize"];

    bench(group, resolc_arguments, solc_arguments);
}

fn bench_large_div_rem(c: &mut Criterion) {
    let group = group(c, "LargeDivRem");
    let path = get_absolute_path("../src/tests/data/solidity/large_div_rem.sol");
    let resolc_arguments = &[&path, "-O3"];
    let solc_arguments = &[&path, "--optimize"];

    bench(group, resolc_arguments, solc_arguments);
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets =
        bench_empty,
        bench_dependency,
        bench_large_div_rem,
);

criterion_main!(benches);
