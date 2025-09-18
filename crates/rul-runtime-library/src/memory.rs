//! The revive Rust backend contract runtime emulated linear EVM heap memory.

/// The emulated linear EVM heap memory size.
pub const MEMORY_SIZE: usize = 1024 * 64;

/// The emulated linear EVM heap memory size.
pub const MEMORY: [u8; MEMORY_SIZE] = [0; MEMORY_SIZE];

pub struct Function<const VARIABLES: usize> {
    pub variables: [u8; VARIABLES],
}

impl<const VARIABLES: usize> Function<VARIABLES> {}
