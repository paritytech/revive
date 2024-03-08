pub mod mock_runtime;

pub fn compile_blob(contract_name: &str, source_code: &str) -> Vec<u8> {
    let file_name = "contract.sol";

    let contracts = revive_solidity::test_utils::build_solidity(
        [(file_name.into(), source_code.into())].into(),
        Default::default(),
        None,
        revive_solidity::SolcPipeline::Yul,
        era_compiler_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("source should compile")
    .contracts
    .expect("source should contain at least one contract");

    let bytecode = contracts[file_name][contract_name]
        .evm
        .as_ref()
        .expect("source should produce EVM output")
        .assembly_text
        .as_ref()
        .expect("source should produce assembly text");

    hex::decode(bytecode).expect("hex encoding should always be valid")
}

#[cfg(test)]
mod tests {
    use primitive_types::U256;

    use crate::mock_runtime::{self, State};

    #[test]
    fn flipper() {
        let source = r#"contract Flipper {
            bool coin;
            function flip() public payable { coin = !coin; }
        }"#;

        let code = crate::compile_blob("Flipper", source);
        let state = State::new(0xcde4efa9u32.to_be_bytes().to_vec());
        let (instance, export) = mock_runtime::prepare(&code);

        let state = crate::mock_runtime::call(state, &instance, export);
        assert_eq!(state.output.flags, 0);
        assert_eq!(state.storage[&U256::zero()], U256::one());

        let state = crate::mock_runtime::call(state, &instance, export);
        assert_eq!(state.output.flags, 0);
        assert_eq!(state.storage[&U256::zero()], U256::zero());
    }
}
