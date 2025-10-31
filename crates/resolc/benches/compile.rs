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

fn bench_overwrite_same_memory(c: &mut Criterion) {
    let contract = UncompiledContract::overwrite_same_memory_n_times(1000);
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
            targets = bench_overwrite_same_memory,
            $(
                $function_name
            ),*
        );
    };
}

create_bench_functions!(
    bench_add_mod_mul_mod => "AddModMulMod.sol",
    bench_balance => "Balance.sol",
    bench_baseline => "Baseline.sol",
    bench_call => "Call.sol",
    bench_computation => "Computation.sol",
    bench_create => "Create.sol",
    bench_create2 => "Create2.sol",
    bench_delegate => "Delegate.sol",
    bench_division_arithmetics => "DivisionArithmetics.sol",
    bench_erc20 => "ERC20.sol",
    bench_events => "Events.sol",
    bench_ext_code => "ExtCode.sol",
    bench_fibonacci => "Fibonacci.sol",
    bench_function_pointer => "FunctionPointer.sol",
    bench_function_type => "FunctionType.sol",
    bench_gas_left => "GasLeft.sol",
    bench_layout_at => "LayoutAt.sol",
    bench_m_copy_overlap => "MCopyOverlap.sol",
    bench_send => "Send.sol",
    bench_sha1 => "SHA1.sol",
    bench_storage => "Storage.sol",
    bench_transfer => "Transfer.sol",
    bench_value => "Value.sol",
);

criterion_main!(compile);
