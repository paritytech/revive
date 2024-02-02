use inkwell::context::Context;

pub mod environment;
pub mod linker;
pub mod target;

pub struct PolkaVm(Context);

impl Default for PolkaVm {
    fn default() -> Self {
        Self(Context::create())
    }
}
