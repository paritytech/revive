//! Process for compiling a single compilation unit.

pub mod input;
#[cfg(not(target_os = "emscripten"))]
pub mod native_process;
pub mod output;
#[cfg(target_os = "emscripten")]
pub mod worker_process;

use self::input::Input;
use self::output::Output;

pub trait Process {
    fn run() -> anyhow::Result<()>;
    fn call(input: Input) -> anyhow::Result<Output>;
}
