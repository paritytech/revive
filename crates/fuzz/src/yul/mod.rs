//! Yul-source differential. Generates Yul objects directly and
//! feeds the same text through both backends, so divergences are
//! purely backend-side (no Solidity frontend involved).
//!
//! Reuses [`Outcome`](crate::observe::Outcome) from the Solidity
//! side for reporting.

pub mod differential;
pub mod generator;
pub mod observe;
pub mod pipeline;

pub use differential::{run_yul_case, YulCompareReport, YulDivergence};
pub use generator::{YulCase, ACTION_CALLDATA_LEN};
