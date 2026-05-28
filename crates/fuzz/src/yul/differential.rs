//! Yul-source differential. Both backends consume the same source
//! (no Solidity frontend involved), so a [`YulDivergence`] is purely
//! backend-side.

use thiserror::Error;

use crate::observe::Outcome;
use crate::yul::generator::YulCase;
use crate::yul::observe::{observe_evm_yul, observe_pvm_yul};
use crate::yul::pipeline::{yul_to_evm, yul_to_pvm};

#[derive(Debug, Error)]
pub enum YulDivergence {
    #[error("yul→evm compile failed: {0}")]
    EvmCompile(String),

    #[error("yul→pvm compile failed: {0}")]
    PvmCompile(String),

    #[error("deploy_reverted mismatch — evm={evm} pvm={pvm}")]
    DeployRevert { evm: bool, pvm: bool },

    /// Defensive — kept so `compare` stays total.
    #[error("action count mismatch — evm={evm} pvm={pvm}")]
    ActionCount { evm: usize, pvm: usize },

    #[error("action[{index}] revert mismatch — evm={evm} pvm={pvm}")]
    ActionRevert { index: usize, evm: bool, pvm: bool },

    #[error("action[{index}] return-data mismatch (lengths evm={a}, pvm={b})")]
    ActionReturnData {
        index: usize,
        a: usize,
        b: usize,
        full: Box<(Vec<u8>, Vec<u8>)>,
    },
}

#[derive(Debug)]
pub struct YulCompareReport {
    pub evm: Outcome,
    pub pvm: Outcome,
}

pub fn run_yul_case(case: &YulCase) -> Result<YulCompareReport, YulDivergence> {
    let evm_bytes = yul_to_evm(&case.contract_name, &case.source)
        .map_err(|error| YulDivergence::EvmCompile(error.to_string()))?;
    let pvm_blob = yul_to_pvm(&case.contract_name, &case.source)
        .map_err(|error| YulDivergence::PvmCompile(error.to_string()))?;

    let evm = observe_evm_yul(evm_bytes, &case.actions);
    let pvm = observe_pvm_yul(pvm_blob, &case.actions);

    compare(&evm, &pvm)?;

    Ok(YulCompareReport { evm, pvm })
}

fn compare(evm: &Outcome, pvm: &Outcome) -> Result<(), YulDivergence> {
    if evm.deploy_reverted != pvm.deploy_reverted {
        return Err(YulDivergence::DeployRevert {
            evm: evm.deploy_reverted,
            pvm: pvm.deploy_reverted,
        });
    }
    if evm.deploy_reverted {
        return Ok(());
    }
    if evm.actions.len() != pvm.actions.len() {
        return Err(YulDivergence::ActionCount {
            evm: evm.actions.len(),
            pvm: pvm.actions.len(),
        });
    }
    for (index, (a, b)) in evm.actions.iter().zip(pvm.actions.iter()).enumerate() {
        if a.reverted != b.reverted {
            return Err(YulDivergence::ActionRevert {
                index,
                evm: a.reverted,
                pvm: b.reverted,
            });
        }
        if a.return_data != b.return_data {
            return Err(YulDivergence::ActionReturnData {
                index,
                a: a.return_data.len(),
                b: b.return_data.len(),
                full: Box::new((a.return_data.clone(), b.return_data.clone())),
            });
        }
    }
    Ok(())
}
