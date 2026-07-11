//! Differential driver: compile both backends, execute, compare.

use thiserror::Error;

use crate::generator::SolidityCase;
use crate::observe::{observe_evm, observe_pvm, Outcome};
use crate::pipeline::{resolc_pvm, solc_evm};

#[derive(Debug, Error)]
pub enum Divergence {
    /// solc rejected the template — generator bug, skipped by libFuzzer.
    #[error("solc EVM compile failed: {0}")]
    EvmCompile(String),

    /// resolc choked on input solc accepted — real backend find.
    #[error("resolc PVM compile failed: {0}")]
    PvmCompile(String),

    #[error("deploy_reverted mismatch — evm={evm} pvm={pvm}")]
    DeployRevert { evm: bool, pvm: bool },

    /// Defensive — both observers push one result per queued action.
    #[error("action count mismatch — evm={evm} pvm={pvm}")]
    ActionCount { evm: usize, pvm: usize },

    #[error("action[{index}] revert mismatch — evm={evm} pvm={pvm}")]
    ActionRevert { index: usize, evm: bool, pvm: bool },

    #[error("action[{index}] return-data mismatch (lengths: evm={a}, pvm={b})")]
    ActionReturnData {
        index: usize,
        a: usize,
        b: usize,
        full: Box<(Vec<u8>, Vec<u8>)>,
    },
}

#[derive(Debug)]
pub struct CompareReport {
    pub evm: Outcome,
    pub pvm: Outcome,
}

/// EVM via direct solc — pure backend-vs-backend, no printer noise.
pub fn run_case_solc_evm(case: &SolidityCase) -> Result<CompareReport, Divergence> {
    let evm_bytes = solc_evm(&case.contract_name, &case.source)
        .map_err(|error| Divergence::EvmCompile(error.to_string()))?;
    let pvm_blob = resolc_pvm(&case.contract_name, &case.source)
        .map_err(|error| Divergence::PvmCompile(error.to_string()))?;

    let evm = observe_evm(evm_bytes, case);
    let pvm = observe_pvm(pvm_blob, case);

    compare(&evm, &pvm)?;

    Ok(CompareReport { evm, pvm })
}

fn compare(evm: &Outcome, pvm: &Outcome) -> Result<(), Divergence> {
    if evm.deploy_reverted != pvm.deploy_reverted {
        return Err(Divergence::DeployRevert {
            evm: evm.deploy_reverted,
            pvm: pvm.deploy_reverted,
        });
    }
    if evm.deploy_reverted {
        return Ok(());
    }
    if evm.actions.len() != pvm.actions.len() {
        return Err(Divergence::ActionCount {
            evm: evm.actions.len(),
            pvm: pvm.actions.len(),
        });
    }
    for (index, (a, b)) in evm.actions.iter().zip(pvm.actions.iter()).enumerate() {
        if a.reverted != b.reverted {
            return Err(Divergence::ActionRevert {
                index,
                evm: a.reverted,
                pvm: b.reverted,
            });
        }
        if a.return_data != b.return_data {
            return Err(Divergence::ActionReturnData {
                index,
                a: a.return_data.len(),
                b: b.return_data.len(),
                full: Box::new((a.return_data.clone(), b.return_data.clone())),
            });
        }
    }
    Ok(())
}
