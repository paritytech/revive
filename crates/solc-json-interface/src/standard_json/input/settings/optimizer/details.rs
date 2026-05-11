//! The `solc --standard-json` input settings optimizer details.

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::input::settings::optimizer::yul_details::YulDetails;

/// Yul optimizer step sequence tuned for PolkaVM code size.
///
/// Same as the solc default sequence, but with an extra `[LScsTulD]` cleanup loop
/// (LoadResolver, UnusedStoreEliminator, CSE, ExpressionSimplifier, LiteralRematerialiser,
/// UnusedPruner, DeadCodeEliminator) appended before the final cleanup colon, which
/// shaves ~174 bytes from the OZ contract suite.
const PVM_YUL_STEPS: &str = "dhfoDgvulfnTUtnIfxa[r]EscLMVcul[j]Trpeulxa[r]cLgvifMCTUca[r]LSsTFOtfDnca[r]IulcscCTUtgvifMx[scCTUt]TOntnfDIulgvifMjmul[jul]VcTOculjmul[LScsTulD]:fDnTOcmuO";

/// The `solc --standard-json` input settings optimizer details.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peephole: Option<bool>,
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inliner: Option<bool>,
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jumpdest_remover: Option<bool>,
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_literals: Option<bool>,
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicate: Option<bool>,
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cse: Option<bool>,
    /// Whether the pass is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constant_optimizer: Option<bool>,
    /// Whether the YUL optimizer is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yul: Option<bool>,
    /// The YUL optimizer configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yul_details: Option<YulDetails>,
}

impl Details {
    /// A shortcut constructor.
    pub fn new(
        peephole: Option<bool>,
        inliner: Option<bool>,
        jumpdest_remover: Option<bool>,
        order_literals: Option<bool>,
        deduplicate: Option<bool>,
        cse: Option<bool>,
        constant_optimizer: Option<bool>,
        yul: Option<bool>,
        yul_details: Option<YulDetails>,
    ) -> Self {
        Self {
            peephole,
            inliner,
            jumpdest_remover,
            order_literals,
            deduplicate,
            cse,
            constant_optimizer,
            yul,
            yul_details,
        }
    }

    /// Optimizer details tuned for PolkaVM code size.
    ///
    /// Enables the Yul optimizer with a step sequence that appends an extra `[LScsTulD]`
    /// cleanup loop to solc's default — see `PVM_YUL_STEPS` for the full sequence and
    /// rationale.
    pub fn pvm_size() -> Self {
        Self {
            yul: Some(true),
            yul_details: Some(YulDetails::new(None, Some(PVM_YUL_STEPS.to_string()))),
            ..Default::default()
        }
    }

    /// Creates disabled optimizer details.
    pub fn disabled(version: &semver::Version) -> Self {
        let inliner = if version >= &semver::Version::new(0, 8, 5) {
            Some(false)
        } else {
            None
        };

        Self::new(
            Some(false),
            inliner,
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            None,
            None,
        )
    }
}
