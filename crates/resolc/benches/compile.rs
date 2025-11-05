#![cfg(feature = "bench-resolc")]

use std::{
    process::Command,
    time::{Duration, Instant},
};

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use revive_integration::cases::UncompiledContract;

fn measure_resolc(arguments: &[&str], iters: u64) -> Duration {
    let start = Instant::now();

    for _i in 0..iters {
        execute_resolc(arguments);
    }

    start.elapsed()
}

#[inline(always)]
fn execute_resolc(arguments: &[&str]) {
    execute_command("resolc", arguments)
}

#[inline(always)]
fn execute_command(command: &str, arguments: &[&str]) {
    Command::new(command)
        .args(arguments)
        .output()
        .expect("command failed");
}

/// Get the relative path to a `.sol` contract file with file stem `name`.
/// The file is expected to live under `"crates/integration/contracts/"`.
fn get_contract_path(name: &str) -> String {
    format!("crates/integration/contracts/{name}.sol")
}

fn bench(mut group: BenchmarkGroup<'_, WallTime>, compiler_arguments: &[&str]) {
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    group.bench_function("Resolc", |b| {
        b.iter_custom(|iters| measure_resolc(compiler_arguments, iters));
    });

    group.finish();
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench_overwrite_same_memory(c: &mut Criterion) {
    let contract = UncompiledContract::overwrite_same_memory_n_times(1000);
    let group = group(c, &contract.name);
    let compiler_arguments = &[&contract.path, "-O0"];

    bench(group, compiler_arguments);
}

fn bench_baseline(c: &mut Criterion) {
    let name = "Baseline";
    let path = get_contract_path(name);
    let group = group(c, name);
    let compiler_arguments = &[&path, "-O0"];

    bench(group, compiler_arguments);
}

fn bench_erc20(c: &mut Criterion) {
    let name = "ERC20";
    let path = get_contract_path(name);
    let group = group(c, name);
    let compiler_arguments = &[&path, "-O0"];

    bench(group, compiler_arguments);
}

fn bench_sha1(c: &mut Criterion) {
    let name = "SHA1";
    let path = get_contract_path(name);
    let group = group(c, name);
    let compiler_arguments = &[&path, "-O0"];

    bench(group, compiler_arguments);
}

fn bench_storage(c: &mut Criterion) {
    let name = "Storage";
    let path = get_contract_path(name);
    let group = group(c, name);
    let compiler_arguments = &[&path, "-O0"];

    bench(group, compiler_arguments);
}

fn bench_transfer(c: &mut Criterion) {
    let name = "Transfer";
    let path = get_contract_path(name);
    let group = group(c, name);
    let compiler_arguments = &[&path, "-O0"];

    bench(group, compiler_arguments);
}

fn bench_value(c: &mut Criterion) {
    let name = "Value";
    let path = get_contract_path(name);
    let group = group(c, name);
    let compiler_arguments = &[&path, "-O0"];

    bench(group, compiler_arguments);
}

criterion_group!(
    name = compile;
    config = Criterion::default();
    targets =
        bench_overwrite_same_memory,
        bench_baseline,
        bench_erc20,
        bench_sha1,
        bench_storage,
        bench_transfer,
        bench_value,
);

criterion_main!(compile);
