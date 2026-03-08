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
        // Run the standard Oz pipeline, then IPSCCP to propagate constants
        // through call boundaries of outlined helpers, followed by
        // inlining (which can now inline functions with constant args)
        // and a cleanup round.
        let pass_pipeline = format!(
            "default<O{opt}>,ipsccp,deadargelim,\
             inline,function(simplifycfg),globaldce"
        );
        target_machine.run_optimization_passes(module, &pass_pipeline)
    }

    /// Returns the optimizer settings reference.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
}
