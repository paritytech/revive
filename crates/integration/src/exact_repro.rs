#[cfg(test)]
mod tests {
    use revive_runner::*;

    fn compile_test_blob() -> Vec<u8> {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function main(uint8 x) public pure returns (uint8) {
        return x;
    }
}
"#;
        resolc::test_utils::compile_blob_with_options(
            "Test",
            source,
            false,
            revive_llvm_context::OptimizerSettings::cycles(),
            Default::default(),
        )
    }

    /// Dump the PVM blob for manual analysis
    #[test]
    fn dump_pvm_blob() {
        let blob = compile_test_blob();
        eprintln!("PVM blob size: {} bytes", blob.len());
        std::fs::write("/tmp/test_uint8_newyork.pvm", &blob).unwrap();
        eprintln!("Wrote to /tmp/test_uint8_newyork.pvm");
    }

    /// Call main(uint8) with 0x100000000 — MUST revert
    #[test]
    fn trace_uint8_validation() {
        let calldata =
            hex::decode("ebbee3910000000000000000000000000000000000000000000000000000000100000000")
                .unwrap();

        Specs {
            differential: false,
            balances: vec![(ALICE, 1_000_000_000_000)],
            actions: vec![
                SpecsAction::Instantiate {
                    origin: TestAddress::Alice,
                    value: 0,
                    gas_limit: Some(GAS_LIMIT),
                    storage_deposit_limit: Some(DEPOSIT_LIMIT),
                    code: Code::Bytes(compile_test_blob()),
                    data: vec![],
                    salt: Default::default(),
                },
                SpecsAction::Call {
                    origin: TestAddress::Alice,
                    dest: TestAddress::Instantiated(0),
                    value: 0,
                    gas_limit: Some(GAS_LIMIT),
                    storage_deposit_limit: Some(DEPOSIT_LIMIT),
                    data: calldata,
                },
                SpecsAction::VerifyCall(VerifyCallExpectation {
                    success: false,
                    gas_consumed: None,
                    output: Default::default(),
                }),
            ],
        }
        .run();
    }

    /// Call main(uint8) with 5 — should succeed
    #[test]
    fn trace_uint8_valid_value() {
        let calldata =
            hex::decode("ebbee3910000000000000000000000000000000000000000000000000000000000000005")
                .unwrap();

        Specs {
            differential: false,
            balances: vec![(ALICE, 1_000_000_000_000)],
            actions: vec![
                SpecsAction::Instantiate {
                    origin: TestAddress::Alice,
                    value: 0,
                    gas_limit: Some(GAS_LIMIT),
                    storage_deposit_limit: Some(DEPOSIT_LIMIT),
                    code: Code::Bytes(compile_test_blob()),
                    data: vec![],
                    salt: Default::default(),
                },
                SpecsAction::Call {
                    origin: TestAddress::Alice,
                    dest: TestAddress::Instantiated(0),
                    value: 0,
                    gas_limit: Some(GAS_LIMIT),
                    storage_deposit_limit: Some(DEPOSIT_LIMIT),
                    data: calldata,
                },
                SpecsAction::VerifyCall(VerifyCallExpectation {
                    success: true,
                    gas_consumed: None,
                    output: Default::default(),
                }),
            ],
        }
        .run();
    }
}
