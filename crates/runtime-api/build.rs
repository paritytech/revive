use std::{env, fs, path::Path, process::Command};

const TARGET_TRIPLE_FLAG: &str = "-triple=riscv64-unknown-unknown-elf";
const TARGET_FLAG: &str = "--target=riscv64";
const TARGET_ARCH_FLAG: &str = "-march=rv64emac";
const TARGET_ABI_FLAG: &str = "-mabi=lp64e";

/// RISC-V backend features enabled for the runtime-API helpers (`memcpy`,
/// `memmove`, `memset` and the syscall stubs).
///
/// Each feature MUST be passed as its own `-target-feature` flag. A single
/// comma-joined value (`+a,+b,...`) does not work: clang treats one
/// `-target-feature` argument as one feature and appends its own
/// `-march`-derived feature list *after* it, so under LLVM's last-one-wins
/// merge the comma blob is silently cancelled — e.g. `+xtheadcondmov` and
/// `+zbb` end up disabled.
///
/// The list mirrors `TargetMachine::VM_FEATURES` in `revive-llvm-context`
/// (kept in sync by hand — a build script can't depend on that crate). It
/// deliberately omits `+unaligned-scalar-mem`: these helpers are linked into
/// every contract and must stay inline-compatible with the **stock** Yul path,
/// whose target machine does not enable that feature (newyork appends it only
/// for its own code). LLVM refuses to inline a callee whose feature set is not
/// a subset of the caller's, so a helper carrying `+unaligned-scalar-mem`
/// would fail to inline into stock-path code. The feature does not change the
/// helpers' own codegen, so omitting it costs nothing.
const TARGET_FEATURES: &[&str] = &[
    "+e",
    "+m",
    "+a",
    "+c",
    "+zbb",
    "+auipc-addi-fusion",
    "+ld-add-fusion",
    "+lui-addi-fusion",
    "+xtheadcondmov",
    "+relax",
];

const IMPORTS_SOUCE: &str = "src/polkavm_imports.c";
const IMPORTS_BC: &str = "polkavm_imports.bc";
const IMPORTS_RUST: &str = "polkavm_imports.rs";

const EXPORTS_SOUCE: &str = "src/polkavm_exports.c";
const EXPORTS_BC: &str = "polkavm_exports.bc";
const EXPORTS_RUST: &str = "polkavm_exports.rs";

fn compile(source_path: &str, bitcode_path: &str) {
    let mut args: Vec<&str> = vec![
        TARGET_FLAG,
        "-Xclang",
        TARGET_TRIPLE_FLAG,
        TARGET_ARCH_FLAG,
        TARGET_ABI_FLAG,
    ];
    for feature in TARGET_FEATURES {
        args.extend(["-Xclang", "-target-feature", "-Xclang", feature]);
    }
    args.extend([
        "-fno-exceptions",
        "-ffreestanding",
        "-Wall",
        "-fno-builtin",
        "-O3",
        "-emit-llvm",
        "-c",
        "-o",
        bitcode_path,
        source_path,
    ]);

    let output = Command::new(revive_build_utils::llvm_host_tool("clang"))
        .args(&args)
        .output()
        .unwrap_or_else(|error| panic!("failed to execute clang: {error}"));

    assert!(
        output.status.success(),
        "failed to compile the PolkaVM C API: {output:?}"
    );
}

fn build_module(source_path: &str, bitcode_path: &str, rust_file: &str) {
    let out_dir = env::var_os("OUT_DIR").expect("env should have $OUT_DIR");
    let lib = Path::new(&out_dir).join(bitcode_path);
    compile(source_path, lib.to_str().expect("$OUT_DIR should be UTF-8"));

    // `inkwell::MemoryBuffer::create_from_memory_range` requires a trailing nul byte
    // (it subtracts one from the length to drop the terminator), so we embed the bitcode
    // with an extra nul byte appended.
    let mut bitcode = fs::read(lib).expect("bitcode should have been built");
    bitcode.push(0);
    let padded_name = format!("{bitcode_path}.nul");
    let padded_path = Path::new(&out_dir).join(&padded_name);
    fs::write(&padded_path, &bitcode).expect("should be able to write in $OUT_DIR");
    let len = bitcode.len();
    let src_path = Path::new(&out_dir).join(rust_file);
    let src = format!("pub static BITCODE: &[u8; {len}] = include_bytes!(\"{padded_name}\");");
    fs::write(src_path, src).expect("should be able to write in $OUT_DIR");
}

fn main() {
    println!(
        "cargo:rerun-if-env-changed={}",
        revive_build_utils::REVIVE_LLVM_HOST_PREFIX
    );

    build_module(IMPORTS_SOUCE, IMPORTS_BC, IMPORTS_RUST);
    build_module(EXPORTS_SOUCE, EXPORTS_BC, EXPORTS_RUST);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/polkavm_imports.c");
    println!("cargo:rerun-if-changed=src/polkavm_exports.c");
    println!("cargo:rerun-if-changed=src/polkavm_guest.h");
}
