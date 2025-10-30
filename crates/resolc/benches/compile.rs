#![cfg(feature = "bench-resolc")]

use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use criterion::{
    criterion_group, criterion_main,
    measurement::{Measurement, WallTime},
    BenchmarkGroup, Criterion,
};
use revive_integration::cases::UncompiledContract;

fn measure_resolc(iters: u64, arguments: &[&str]) -> Duration {
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

fn bench(mut group: BenchmarkGroup<'_, WallTime>, compiler_arguments: &[&str]) {
    group.sample_size(10);

    group.bench_function("Resolc", |b| {
        b.iter_custom(|iters| measure_resolc(iters, compiler_arguments));
    });

    group.finish();
}

fn group<'error, M>(c: &'error mut Criterion<M>, group_name: &str) -> BenchmarkGroup<'error, M>
where
    M: Measurement,
{
    c.benchmark_group(group_name)
}

fn bench_store_uint256(c: &mut Criterion) {
    let contract = UncompiledContract::store_uint256_n_times(1000);
    let group = group(c, &contract.name);
    let compiler_arguments = &[&contract.path, "-O0"];

    bench(group, compiler_arguments);
}

fn get_file_stem(path: &str) -> &str {
    Path::new(path).file_stem().unwrap().to_str().unwrap()
}

/// Creates benchmark functions which are added as targets
/// to the `criterion_group` macro.
///
/// ## Parameters
/// 1. The generated function name => The filename
///    (filename is expected to live under `"../../integration/contracts/"`)
///
/// ## Examples
/// ```rust
/// create_bench_functions!(
///     bench_ext_code => "ExtCode.sol"
///     bench_fibonacci => "Fibonacci.sol"
/// );
/// // Generates:
/// fn bench_ext_code(c: &mut Criterion) {/*...*/}
/// fn bench_fibonacci(c: &mut Criterion) {/*...*/}
/// criterion_group!(
///     /*...*/
///     targets =
///     bench_ext_code,
///     bench_fibonacci
/// );
/// ```
macro_rules! create_bench_functions {
    ($($function_name:ident => $filename:expr),+) => {
        $(
            fn $function_name(c: &mut Criterion) {
                let path = concat!("../../integration/contracts/", $filename);
                let group = group(c, get_file_stem(path));
                let compiler_arguments = &[path, "-O0"];

                bench(group, compiler_arguments);
            }
        )*

        criterion_group!(
            name = compile;
            config = Criterion::default();
            targets = bench_store_uint256,
            $(
                $function_name
            ),*
        );
    };
}

create_bench_functions!(
    bench_add_mod_mul_mod => "AddModMulMod",
    bench_balance => "Balance",
    bench_baseline => "Baseline",
    bench_call => "Call",
    bench_computation => "Computation",
    bench_create => "Create",
    bench_create2 => "Create2",
    bench_delegate => "Delegate",
    bench_division_arithmetics => "DivisionArithmetics",
    bench_erc20 => "ERC20",
    bench_events => "Events",
    bench_ext_code => "ExtCode",
    bench_fibonacci => "Fibonacci",
    bench_function_pointer => "FunctionPointer",
    bench_function_type => "FunctionType",
    bench_gas_left => "GasLeft",
    bench_layout_at => "LayoutAt",
    bench_m_copy_overlap => "MCopyOverlap",
    bench_send => "Send",
    bench_sha1 => "SHA1",
    bench_storage => "Storage",
    bench_transfer => "Transfer",
    bench_value => "Value"
);

criterion_main!(compile);
