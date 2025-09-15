//! The LLVM optimizer settings.

pub mod size_level;

use revive_solc_json_interface::SolcStandardJsonInputSettingsOptimizer;
use serde::Deserialize;
use serde::Serialize;

use itertools::Itertools;

use self::size_level::SizeLevel;

/// The LLVM optimizer and code-gen settings.
#[derive(Debug, Serialize, Deserialize, Clone, Eq)]
pub struct Settings {
    /// The middle-end optimization level.
    pub level_middle_end: inkwell::OptimizationLevel,
    /// The middle-end size optimization level.
    pub level_middle_end_size: SizeLevel,
    /// The back-end optimization level.
    pub level_back_end: inkwell::OptimizationLevel,

    /// Fallback to optimizing for size if the bytecode is too large.
    pub is_fallback_to_size_enabled: bool,

    /// Whether the LLVM `verify each` option is enabled.
    pub is_verify_each_enabled: bool,
    /// Whether the LLVM `debug logging` option is enabled.
    pub is_debug_logging_enabled: bool,
}

impl Settings {
    /// A shortcut constructor.
    pub fn new(
        level_middle_end: inkwell::OptimizationLevel,
        level_middle_end_size: SizeLevel,
        level_back_end: inkwell::OptimizationLevel,
    ) -> Self {
        Self {
            level_middle_end,
            level_middle_end_size,
            level_back_end,

            is_fallback_to_size_enabled: false,

            is_verify_each_enabled: false,
            is_debug_logging_enabled: false,
        }
    }

    /// A shortcut constructor with debugging tools.
    pub fn new_debug(
        level_middle_end: inkwell::OptimizationLevel,
        level_middle_end_size: SizeLevel,
        level_back_end: inkwell::OptimizationLevel,

        is_verify_each_enabled: bool,
        is_debug_logging_enabled: bool,
    ) -> Self {
        Self {
            level_middle_end,
            level_middle_end_size,
            level_back_end,

            is_fallback_to_size_enabled: false,

            is_verify_each_enabled,
            is_debug_logging_enabled,
        }
    }

    /// Creates settings from a CLI optimization parameter.
    pub fn try_from_cli(value: char) -> anyhow::Result<Self> {
        Ok(match value {
            '0' => Self::new(
                // The middle-end optimization level.
                inkwell::OptimizationLevel::None,
                // The middle-end size optimization level.
                SizeLevel::Zero,
                // The back-end optimization level.
                inkwell::OptimizationLevel::None,
            ),
            '1' => Self::new(
                inkwell::OptimizationLevel::Less,
                SizeLevel::Zero,
                // The back-end does not currently distinguish between O1, O2, and O3.
                inkwell::OptimizationLevel::Less,
            ),
            '2' => Self::new(
                inkwell::OptimizationLevel::Default,
                SizeLevel::Zero,
                // The back-end does not currently distinguish between O1, O2, and O3.
                inkwell::OptimizationLevel::Default,
            ),
            '3' => Self::new(
                inkwell::OptimizationLevel::Aggressive,
                SizeLevel::Zero,
                inkwell::OptimizationLevel::Aggressive,
            ),
            's' => Self::new(
                // The middle-end optimization level is ignored when SizeLevel is set.
                inkwell::OptimizationLevel::Default,
                SizeLevel::S,
                inkwell::OptimizationLevel::Aggressive,
            ),
            'z' => Self::new(
                // The middle-end optimization level is ignored when SizeLevel is set.
                inkwell::OptimizationLevel::Default,
                SizeLevel::Z,
                inkwell::OptimizationLevel::Aggressive,
            ),
            char => anyhow::bail!("Unexpected optimization option '{}'", char),
        })
    }

    /// Returns the settings without optimizations.
    pub fn none() -> Self {
        Self::new(
            inkwell::OptimizationLevel::None,
            SizeLevel::Zero,
            inkwell::OptimizationLevel::None,
        )
    }

    /// Returns the settings for the optimal number of VM execution cycles.
    pub fn cycles() -> Self {
        Self::new(
            inkwell::OptimizationLevel::Aggressive,
            SizeLevel::Zero,
            inkwell::OptimizationLevel::Aggressive,
        )
    }

    /// Returns the settings for the optimal size.
    pub fn size() -> Self {
        Self::new(
            inkwell::OptimizationLevel::Default,
            SizeLevel::Z,
            inkwell::OptimizationLevel::Aggressive,
        )
    }

    /// Returns the middle-end optimization parameter as string.
    pub fn middle_end_as_string(&self) -> String {
        match self.level_middle_end_size {
            SizeLevel::Zero => (self.level_middle_end as u8).to_string(),
            size_level => size_level.to_string(),
        }
    }

    /// Checks whether there are middle-end optimizations enabled.
    pub fn is_middle_end_enabled(&self) -> bool {
        self.level_middle_end != inkwell::OptimizationLevel::None
            || self.level_middle_end_size != SizeLevel::Zero
    }

    /// Returns all possible combinations of the optimizer settings.
    /// Used only for testing purposes.
    pub fn combinations() -> Vec<Self> {
        let performance_combinations: Vec<Self> = vec![
            inkwell::OptimizationLevel::None,
            inkwell::OptimizationLevel::Less,
            inkwell::OptimizationLevel::Default,
            inkwell::OptimizationLevel::Aggressive,
        ]
        .into_iter()
        .cartesian_product(vec![
            inkwell::OptimizationLevel::None,
            inkwell::OptimizationLevel::Aggressive,
        ])
        .map(|(optimization_level_middle, optimization_level_back)| {
            Self::new(
                optimization_level_middle,
                SizeLevel::Zero,
                optimization_level_back,
            )
        })
        .collect();

        let size_combinations: Vec<Self> = vec![SizeLevel::S, SizeLevel::Z]
            .into_iter()
            .cartesian_product(vec![
                inkwell::OptimizationLevel::None,
                inkwell::OptimizationLevel::Aggressive,
            ])
            .map(|(size_level, optimization_level_back)| {
                Self::new(
                    inkwell::OptimizationLevel::Default,
                    size_level,
                    optimization_level_back,
                )
            })
            .collect();

        let mut combinations = performance_combinations;
        combinations.extend(size_combinations);

        combinations
    }

    /// Sets the fallback to optimizing for size if the bytecode is too large.
    pub fn enable_fallback_to_size(&mut self) {
        self.is_fallback_to_size_enabled = true;
    }

    /// Whether the fallback to optimizing for size is enabled.
    pub fn is_fallback_to_size_enabled(&self) -> bool {
        self.is_fallback_to_size_enabled
    }
}

impl PartialEq for Settings {
    fn eq(&self, other: &Self) -> bool {
        self.level_middle_end == other.level_middle_end
            && self.level_middle_end_size == other.level_middle_end_size
            && self.level_back_end == other.level_back_end
    }
}

impl std::fmt::Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "M{}B{}",
            self.middle_end_as_string(),
            self.level_back_end as u8,
        )
    }
}
