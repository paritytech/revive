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
    use alloy_primitives::U256;

    use crate::mock_runtime::{self, State};

    #[test]
    fn flipper() {
        let code = crate::compile_blob("Flipper", include_str!("../contracts/flipper.sol"));
        let state = State::new(0xcde4efa9u32.to_be_bytes().to_vec());
        let (instance, export) = mock_runtime::prepare(&code);

        let state = crate::mock_runtime::call(state, &instance, export);
        assert_eq!(state.output.flags, 0);
        assert_eq!(state.storage[&U256::ZERO], U256::try_from(1).unwrap());

        let state = crate::mock_runtime::call(state, &instance, export);
        assert_eq!(state.output.flags, 0);
        assert_eq!(state.storage[&U256::ZERO], U256::ZERO);
    }

    #[test]
    fn erc20() {
        let _ = crate::compile_blob("ERC20", include_str!("../contracts/ERC20.sol"));
    }

    #[test]
    fn triangle_number() {
        let code = crate::compile_blob("Computation", include_str!("../contracts/Computation.sol"));
        let param = alloy_primitives::U256::try_from(3_000_000i64).unwrap();
        let expected = alloy_primitives::U256::try_from(4_500_001_500_000i64).unwrap();

        //  function triangle_number(int64)
        let mut input = 0x0f760610u32.to_be_bytes().to_vec();
        input.extend_from_slice(&param.to_be_bytes::<32>());

        let state = State::new(input);
        let (instance, export) = mock_runtime::prepare(&code);
        let state = crate::mock_runtime::call(state, &instance, export);

        assert_eq!(state.output.flags, 0);

        let received =
            alloy_primitives::U256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
        assert_eq!(received, expected);
    }
}
