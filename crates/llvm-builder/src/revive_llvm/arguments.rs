//! The revive LLVM builder arguments.

use clap::Parser;
use revive_llvm_builder::ccache_variant::CcacheVariant;

/// The revive LLVM builder arguments.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub enum Arguments {
    /// Clone the branch specified in `LLVM.lock`.
    Clone {
        /// Clone with full commits history.
        #[arg(long)]
        deep: bool,

        /// Target environment to build LLVM (gnu, musl, emscripten).
        #[arg(long, default_value_t = revive_llvm_builder::target_env::TargetEnv::GNU)]
        target_env: revive_llvm_builder::target_env::TargetEnv,
    },

    /// Build the LLVM framework.
    Build {
        /// LLVM build type (`Debug`, `Release`, `RelWithDebInfo`, or `MinSizeRel`).
        #[arg(long, default_value_t = revive_llvm_builder::BuildType::Release)]
        build_type: revive_llvm_builder::BuildType,

        /// Target environment to build LLVM (`gnu` or `musl`).
        #[arg(long, default_value = "gnu")]
        target_env: revive_llvm_builder::target_env::TargetEnv,

        /// Additional targets to build LLVM with.
        #[arg(long)]
        targets: Vec<String>,

        /// LLVM projects to build LLVM with.
        #[arg(long)]
        llvm_projects: Vec<revive_llvm_builder::llvm_project::LLVMProject>,

        /// Whether to build LLVM with run-time type information (RTTI) enabled.
        #[arg(long)]
        enable_rtti: bool,

        /// The default target to build LLVM with.
        #[arg(long)]
        default_target: Option<revive_llvm_builder::target_triple::TargetTriple>,

        /// Whether to build the LLVM tests.
        #[arg(long)]
        enable_tests: bool,

        /// Whether to build LLVM for source-based code coverage.
        #[arg(long)]
        enable_coverage: bool,

        /// Extra arguments to pass to CMake.  
        /// A leading backslash will be unescaped.
        #[arg(long)]
        extra_args: Vec<String>,

        /// Whether to use compiler cache (ccache) to speed-up builds.
        #[arg(long)]
        ccache_variant: Option<CcacheVariant>,

        /// Whether to build with assertions enabled or not.
        #[arg(long)]
        enable_assertions: bool,

        /// Build LLVM with sanitizer enabled (`Address`, `Memory`, `MemoryWithOrigins`, `Undefined`, `Thread`, `DataFlow`, or `Address;Undefined`).
        #[arg(long)]
        sanitizer: Option<revive_llvm_builder::sanitizer::Sanitizer>,

        /// Whether to run LLVM unit tests under valgrind or not.
        #[arg(long)]
        enable_valgrind: bool,
    },

    /// Checkout the branch specified in `LLVM.lock`.
    Checkout {
        /// Remove all artifacts preventing the checkout (removes all local changes!).
        #[structopt(long = "force")]
        force: bool,
    },

    /// Clean the build artifacts.
    Clean,

    /// Build the LLVM compiler-rt builtins for the PolkaVM target.
    Builtins {
        /// LLVM build type (`Debug`, `Release`, `RelWithDebInfo`, or `MinSizeRel`).
        #[arg(long, default_value_t = revive_llvm_builder::BuildType::Release)]
        build_type: revive_llvm_builder::BuildType,

        /// The default target to build LLVM with.
        #[arg(long)]
        default_target: Option<revive_llvm_builder::target_triple::TargetTriple>,

        /// Extra arguments to pass to CMake.  
        /// A leading backslash will be unescaped.
        #[arg(long)]
        extra_args: Vec<String>,

        /// Whether to use compiler cache (ccache) to speed-up builds.
        #[arg(long)]
        ccache_variant: Option<CcacheVariant>,

        /// Build LLVM with sanitizer enabled (`Address`, `Memory`, `MemoryWithOrigins`, `Undefined`, `Thread`, `DataFlow`, or `Address;Undefined`).
        #[arg(long)]
        sanitizer: Option<revive_llvm_builder::sanitizer::Sanitizer>,
    },
}
