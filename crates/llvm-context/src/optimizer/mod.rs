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
        // Two-phase pipeline: default<Oz> for initial size optimization,
        // then IPSCCP to propagate inter-function constants through
        // outlined helpers, deadargelim to remove now-constant arguments,
        // and a second default<O1> pass to re-optimize with newly
        // discovered constants.  O1 is used (not Oz) to avoid aggressive
        // LICM of i256 operations that hurts loop-heavy contracts on the
        // 32-bit PVM target.
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
