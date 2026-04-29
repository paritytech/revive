//! The LLVM optimizing tools.

use serde::Deserialize;
use serde::Serialize;

use crate::target_machine::TargetMachine;

use self::settings::Settings;

pub mod settings;

/// The LLVM optimizing tools.
#[derive(Debug, Serialize, Deserialize)]
pub struct Optimizer {
    /// The optimizer settings.
    settings: Settings,
}

impl Optimizer {
    /// A shortcut constructor.
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// Runs the new pass manager.
    pub fn run(
        &self,
        target_machine: &TargetMachine,
        module: &inkwell::module::Module,
    ) -> Result<(), inkwell::support::LLVMString> {
        let opt = self.settings.middle_end_as_string();
        // Two-phase pipeline:
        // 1. default<O{opt}>: initial optimization at the configured size level.
        // 2. IPSCCP + deadargelim: propagate inter-function constants through
        //    outlined helpers and remove now-constant arguments.
        // 3. default<O1>: re-optimize with newly discovered constants.
        //
        // The second pass is hard-coded to `O1` for empirical reasons. The
        // earlier comment claimed `O1` avoids aggressive LICM of i256 — that
        // rationale is circular: LICM promotion and machine LICM are already
        // disabled at the LLVM driver level via `--disable-licm-promotion`
        // and `--disable-machine-licm` in `initialize_llvm`, so neither
        // preset can re-introduce them.
        //
        // The actual reason `O1` is retained is a differential-test
        // regression: matching the second pass to the configured level
        // (running `default<Oz>` after IPSCCP at default size optimization)
        // changes the post-link PVM blob for two contracts under solc M0
        //
        //   * complex/defi/UniswapV2Router01/UniswapV2Pair  (Y M0 S+)
        //   * complex/solidity_by_example/applications/iterable_mapping
        //     (Y M0 S- >=0.8.1)
        //
        // and the changed blobs reproducibly hit a "Code upload failed:
        // Timeout when retrying request" error on pallet-revive deployment
        // (verified across two retester runs at each commit). The exact
        // pass that produces the regression has not been isolated; until
        // it is, keeping the second pass at `O1` is the safe choice.
        let pass_pipeline = format!(
            "default<O{opt}>,ipsccp,deadargelim,\
             default<O1>"
        );
        target_machine.run_optimization_passes(module, &pass_pipeline)
    }

    /// Returns the optimizer settings reference.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
}
