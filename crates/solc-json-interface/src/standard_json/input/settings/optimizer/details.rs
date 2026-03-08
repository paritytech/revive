//! The `solc --standard-json` input settings optimizer details.

use serde::Deserialize;
use serde::Serialize;

use crate::standard_json::input::settings::optimizer::yul_details::YulDetails;

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
    /// Appends an extra `[LScsTulD]` loop to the default solc Yul optimizer
    /// sequence. This adds a final round of LoadResolver, UnusedStoreEliminator,
    /// CSE, ExpressionSimplifier, LiteralRematerialiser, UnusedPruner, and
    /// DeadCodeEliminator that reduces code size by ~174 bytes on OZ contracts.
    pub fn for_polkavm() -> Self {
        // The solc default Yul sequence with an extra cleanup loop appended.
        // Default: dhfoDgvulfnTUtnIfxa[r]EscLMVcul [j]Trpeulxa[r]cLgvifMCTUca[r]
        //          LSsTFOtfDnca[r]IulcscCTUtgvifMx[scCTUt]TOntnfDIulgvifMjmul[jul]
        //          VcTOcul jmul:fDnTOcmuO
        // Added:   [LScsTulD] before the cleanup colon
        let steps = "dhfoDgvulfnTUtnIfxa[r]EscLMVcul \
                     [j]Trpeulxa[r]cLgvifMCTUca[r]LSsTFOtfDnca[r]\
                     IulcscCTUtgvifMx[scCTUt]TOntnfDIulgvifMjmul[jul]\
                     VcTOcul jmul[LScsTulD]:fDnTOcmuO"
            .to_string();
        Self {
            yul: Some(true),
            yul_details: Some(YulDetails::new(None, Some(steps))),
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
