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
        // 1. default<Oz>: initial size optimization
        // 2. IPSCCP + deadargelim: propagate inter-function constants through
        //    outlined helpers and remove now-constant arguments
        // 3. default<O1>: re-optimize with newly discovered constants.
        //    O1 (not Oz) avoids aggressive LICM of i256 operations that hurts
        //    loop-heavy contracts on the 32-bit PVM target.
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
