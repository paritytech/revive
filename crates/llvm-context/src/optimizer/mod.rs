//! The LLVM optimizing tools.

pub mod settings;

use serde::Deserialize;
use serde::Serialize;

use crate::target_machine::TargetMachine;

use self::settings::Settings;

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
        target_machine.run_optimization_passes(
            module,
            format!("default<O{}>", self.settings.middle_end_as_string()).as_str(),
        )
    }

    /// Returns the optimizer settings reference.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
}
