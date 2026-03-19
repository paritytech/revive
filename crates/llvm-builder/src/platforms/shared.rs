//! The shared options for building various platforms.

use crate::ccache_variant::CcacheVariant;
use crate::sanitizer::Sanitizer;
use crate::target_env::TargetEnv;
use crate::target_triple::TargetTriple;
use std::path::Path;
use std::process::Command;

/// The build options shared by all platforms.
pub const SHARED_BUILD_OPTS: [&str; 22] = [
    "-DPACKAGE_VENDOR='Parity Technologies'",
    "-DCMAKE_CXX_FLAGS='-include cstdint -include stdint.h'",
    "-DCMAKE_BUILD_WITH_INSTALL_RPATH=1",
    "-DLLVM_BUILD_DOCS='Off'",
    "-DLLVM_INCLUDE_DOCS='Off'",
    "-DLLVM_INCLUDE_BENCHMARKS='Off'",
    "-DLLVM_INCLUDE_EXAMPLES='Off'",
    "-DLLVM_ENABLE_DOXYGEN='Off'",
    "-DLLVM_ENABLE_SPHINX='Off'",
    "-DLLVM_ENABLE_OCAMLDOC='Off'",
    "-DLLVM_ENABLE_ZLIB='Off'",
    "-DLLVM_ENABLE_ZSTD='Off'",
    "-DLLVM_ENABLE_LIBXML2='Off'",
    "-DLLVM_ENABLE_BINDINGS='Off'",
    "-DLLVM_ENABLE_TERMINFO='Off'",
    "-DLLVM_ENABLE_LIBEDIT='Off'",
    "-DLLVM_ENABLE_LIBPFM='Off'",
    "-DCMAKE_EXPORT_COMPILE_COMMANDS='On'",
    "-DPython3_FIND_REGISTRY='LAST'", // Use Python version from $PATH, not from registry
    "-DBUG_REPORT_URL='https://github.com/paritytech/contract-issues/issues/'",
    "-DCLANG_ENABLE_ARCMT='Off'",
    "-DCLANG_ENABLE_STATIC_ANALYZER='Off'",
];

/// The build options shared by all platforms except MUSL.
pub const SHARED_BUILD_OPTS_NOT_MUSL: [&str; 4] = [
    "-DLLVM_OPTIMIZED_TABLEGEN='On'",
    "-DLLVM_BUILD_RUNTIME='Off'",
    "-DLLVM_BUILD_RUNTIMES='Off'",
    "-DLLVM_INCLUDE_RUNTIMES='Off'",
];

/// The shared build options to treat warnings as errors.
///
/// Disabled because it makes the build very brittle.
pub fn shared_build_opts_werror(_target_env: TargetEnv) -> Vec<String> {
    vec!["-DLLVM_ENABLE_WERROR='Off'".to_string()]
}

/// The build options to set the default target.
pub fn shared_build_opts_default_target(target: Option<TargetTriple>) -> Vec<String> {
    match target {
        Some(target) => vec![format!(
            "-DLLVM_DEFAULT_TARGET_TRIPLE='{}'",
            target.to_string()
        )],
        None => vec![format!(
            "-DLLVM_DEFAULT_TARGET_TRIPLE='{}'",
            TargetTriple::PolkaVM
        )],
    }
}

/// The `musl` building sequence.
pub fn build_musl(build_directory: &Path, target_directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(build_directory)?;
    std::fs::create_dir_all(target_directory)?;

    crate::utils::command(
        Command::new("../configure")
            .current_dir(build_directory)
            .arg(format!("--prefix={}", target_directory.to_string_lossy()))
            .arg(format!(
                "--syslibdir={}/lib/",
                target_directory.to_string_lossy()
            ))
            .arg("--enable-wrapper='clang'"),
        "MUSL configuring",
    )?;
    crate::utils::command(
        Command::new("make")
            .current_dir(build_directory)
            .arg("-j")
            .arg(num_cpus::get().to_string()),
        "MUSL building",
    )?;
    crate::utils::command(
        Command::new("make")
            .current_dir(build_directory)
            .arg("install"),
        "MUSL installing",
    )?;

    let mut include_directory = target_directory.to_path_buf();
    include_directory.push("include/");

    let mut asm_include_directory = include_directory.clone();
    asm_include_directory.push("asm/");
    std::fs::create_dir_all(asm_include_directory.as_path())?;

    let mut types_header_path = asm_include_directory.clone();
    types_header_path.push("types.h");

    let copy_options = fs_extra::dir::CopyOptions {
        overwrite: true,
        copy_inside: true,
        ..Default::default()
    };
    fs_extra::dir::copy("/usr/include/linux", include_directory, &copy_options)?;

    let copy_options = fs_extra::dir::CopyOptions {
        overwrite: true,
        copy_inside: true,
        content_only: true,
        ..Default::default()
    };
    fs_extra::dir::copy(
        "/usr/include/asm-generic",
        asm_include_directory,
        &copy_options,
    )?;

    crate::utils::command(
        Command::new("sed")
            .arg("-i")
            .arg("s/asm-generic/asm/")
            .arg(types_header_path),
        "types_header asm signature replacement",
    )?;

    Ok(())
}

/// The build options to enable assertions.
pub fn shared_build_opts_assertions(enabled: bool) -> Vec<String> {
    vec![format!(
        "-DLLVM_ENABLE_ASSERTIONS='{}'",
        if enabled { "On" } else { "Off" },
    )]
}

/// The build options to build with RTTI support.
pub fn shared_build_opts_rtti(enabled: bool) -> Vec<String> {
    vec![format!(
        "-DLLVM_ENABLE_RTTI='{}'",
        if enabled { "On" } else { "Off" },
    )]
}

/// The build options to enable sanitizers.
pub fn shared_build_opts_sanitizers(sanitizer: Option<Sanitizer>) -> Vec<String> {
    match sanitizer {
        Some(sanitizer) => vec![format!("-DLLVM_USE_SANITIZER='{}'", sanitizer)],
        None => vec![],
    }
}

/// The build options to enable Valgrind for LLVM regression tests.
pub fn shared_build_opts_valgrind(enabled: bool) -> Vec<String> {
    if enabled {
        vec!["-DLLVM_LIT_ARGS='-sv --vg --vg-leak'".to_owned()]
    } else {
        vec![]
    }
}

/// The LLVM tests build options shared by all platforms.
pub fn shared_build_opts_tests(enabled: bool) -> Vec<String> {
    vec![
        format!(
            "-DLLVM_BUILD_UTILS='{}'",
            if enabled { "On" } else { "Off" },
        ),
        format!(
            "-DLLVM_BUILD_TESTS='{}'",
            if enabled { "On" } else { "Off" },
        ),
        format!(
            "-DLLVM_INCLUDE_UTILS='{}'",
            if enabled { "On" } else { "Off" },
        ),
        format!(
            "-DLLVM_INCLUDE_TESTS='{}'",
            if enabled { "On" } else { "Off" },
        ),
    ]
}

/// The code coverage build options shared by all platforms.
pub fn shared_build_opts_coverage(enabled: bool) -> Vec<String> {
    vec![format!(
        "-DLLVM_BUILD_INSTRUMENTED_COVERAGE='{}'",
        if enabled { "On" } else { "Off" },
    )]
}

/// Use of compiler cache (ccache) to speed up the build process.
pub fn shared_build_opts_ccache(ccache_variant: Option<CcacheVariant>) -> Vec<String> {
    match ccache_variant {
        Some(ccache_variant) => vec![
            format!(
                "-DCMAKE_C_COMPILER_LAUNCHER='{}'",
                ccache_variant.to_string()
            ),
            format!(
                "-DCMAKE_CXX_COMPILER_LAUNCHER='{}'",
                ccache_variant.to_string()
            ),
        ],
        None => vec![],
    }
}

/// Ignore duplicate libraries warnings for MacOS with XCode>=15.
pub fn macos_build_opts_ignore_dupicate_libs_warnings() -> Vec<String> {
    let xcode_version =
        crate::utils::get_xcode_version().unwrap_or(crate::utils::XCODE_MIN_VERSION);
    if xcode_version >= crate::utils::XCODE_VERSION_15 {
        vec![
            "-DCMAKE_EXE_LINKER_FLAGS='-Wl,-no_warn_duplicate_libraries'".to_owned(),
            "-DCMAKE_SHARED_LINKER_FLAGS='-Wl,-no_warn_duplicate_libraries'".to_owned(),
        ]
    } else {
        vec![]
    }
}
