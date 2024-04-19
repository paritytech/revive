pub mod cases;
pub mod mock_runtime;

/// Compile the blob of `contract_name` found in given `source_code`.
/// The `solc` optimizer will be enabled
pub fn compile_blob(contract_name: &str, source_code: &str) -> Vec<u8> {
    compile_blob_with_options(
        contract_name,
        source_code,
        true,
        revive_solidity::SolcPipeline::Yul,
    )
}

/// Compile the blob of `contract_name` found in given `source_code`.
pub fn compile_blob_with_options(
    contract_name: &str,
    source_code: &str,
    solc_optimizer_enabled: bool,
    pipeline: revive_solidity::SolcPipeline,
) -> Vec<u8> {
    let file_name = "contract.sol";

    let contracts = revive_solidity::test_utils::build_solidity_with_options(
        [(file_name.into(), source_code.into())].into(),
        Default::default(),
        None,
        pipeline,
        era_compiler_llvm_context::OptimizerSettings::cycles(),
        solc_optimizer_enabled,
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
    use alloy_primitives::{FixedBytes, Keccak256, I256, U256};
    use alloy_sol_types::{sol, SolCall};
    use sha1::Digest;

    use crate::{
        cases::Contract,
        mock_runtime::{self, State},
    };

    #[test]
    fn fibonacci() {
        let parameter = 6;
        for contract in [
            Contract::fib_recursive(parameter),
            Contract::fib_iterative(parameter),
            Contract::fib_binet(parameter),
        ] {
            let state = State::new(contract.calldata);
            let (mut instance, export) = mock_runtime::prepare(&contract.pvm_runtime, None);
            let state = crate::mock_runtime::call(state, &mut instance, export);
            assert_eq!(state.output.flags, 0);

            let received = U256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
            let expected = U256::from(8);
            assert_eq!(received, expected);
        }
    }

    #[test]
    fn flipper() {
        let code = crate::compile_blob("Flipper", include_str!("../contracts/flipper.sol"));
        let state = State::new(0xcde4efa9u32.to_be_bytes().to_vec());
        let (mut instance, export) = mock_runtime::prepare(&code, None);

        let state = crate::mock_runtime::call(state, &mut instance, export);
        assert_eq!(state.output.flags, 0);
        assert_eq!(state.storage[&U256::ZERO], U256::try_from(1).unwrap());

        let state = crate::mock_runtime::call(state, &mut instance, export);
        assert_eq!(state.output.flags, 0);
        assert_eq!(state.storage[&U256::ZERO], U256::ZERO);
    }

    #[test]
    fn hash_keccak_256() {
        sol!(
            #[derive(Debug, PartialEq, Eq)]
            contract TestSha3 {
                function test(string memory _pre) external payable returns (bytes32);
            }
        );
        let source = r#"contract TestSha3 {
            function test(string memory _pre) external payable returns (bytes32 hash) {
                hash = keccak256(bytes(_pre));
            }
        }"#;
        let code = crate::compile_blob("TestSha3", source);

        let param = "hello";
        let input = TestSha3::testCall::new((param.to_string(),)).abi_encode();

        let state = State::new(input);
        let (mut instance, export) = mock_runtime::prepare(&code, None);
        let state = crate::mock_runtime::call(state, &mut instance, export);

        assert_eq!(state.output.flags, 0);

        let mut hasher = Keccak256::new();
        hasher.update(param);
        let expected = hasher.finalize();
        let received = FixedBytes::<32>::from_slice(&state.output.data);
        assert_eq!(received, expected);
    }

    #[test]
    fn erc20() {
        let _ = crate::compile_blob("ERC20", include_str!("../contracts/ERC20.sol"));
    }

    #[test]
    fn triangle_number() {
        let contract = Contract::triangle_number(13);
        let state = State::new(contract.calldata);
        let (mut instance, export) = mock_runtime::prepare(&contract.pvm_runtime, None);
        let state = crate::mock_runtime::call(state, &mut instance, export);
        assert_eq!(state.output.flags, 0);

        let received = U256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
        let expected = U256::try_from(91).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn odd_product() {
        let contract = Contract::odd_product(5);
        let state = State::new(contract.calldata);
        let (mut instance, export) = mock_runtime::prepare(&contract.pvm_runtime, None);
        let state = crate::mock_runtime::call(state, &mut instance, export);
        assert_eq!(state.output.flags, 0);

        let received = I256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
        let expected = I256::try_from(945i64).unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn msize_plain() {
        sol!(
            #[derive(Debug, PartialEq, Eq)]
            contract MSize {
                function mSize() public pure returns (uint);
            }
        );
        let code = crate::compile_blob_with_options(
            "MSize",
            include_str!("../contracts/MSize.sol"),
            false,
            revive_solidity::SolcPipeline::EVMLA,
        );
        let (mut instance, export) = mock_runtime::prepare(&code, None);

        let input = MSize::mSizeCall::new(()).abi_encode();
        let state = crate::mock_runtime::call(State::new(input), &mut instance, export);

        assert_eq!(state.output.flags, 0);

        // Solidity always stores the "free memory pointer" (32 byte int) at offset 64.
        let expected = U256::try_from(64 + 32).unwrap();
        let received = U256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
        assert_eq!(received, expected);
    }

    #[test]
    fn transferred_value() {
        sol!(
            contract Value {
                function value() public payable returns (uint);
            }
        );
        let code = crate::compile_blob("Value", include_str!("../contracts/Value.sol"));
        let mut state = State::new(Value::valueCall::SELECTOR.to_vec());
        state.value = 0x1;

        let (mut instance, export) = mock_runtime::prepare(&code, None);
        let state = crate::mock_runtime::call(state, &mut instance, export);

        assert_eq!(state.output.flags, 0);

        let expected = I256::try_from(state.value).unwrap();
        let received = I256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
        assert_eq!(received, expected);
    }

    #[test]
    fn msize_non_word_sized_access() {
        sol!(
            #[derive(Debug, PartialEq, Eq)]
            contract MSize {
                function mStore100() public pure returns (uint);
            }
        );
        let code = crate::compile_blob_with_options(
            "MSize",
            include_str!("../contracts/MSize.sol"),
            false,
            revive_solidity::SolcPipeline::Yul,
        );
        let (mut instance, export) = mock_runtime::prepare(&code, None);

        let input = MSize::mStore100Call::new(()).abi_encode();
        let state = crate::mock_runtime::call(State::new(input), &mut instance, export);

        assert_eq!(state.output.flags, 0);

        // https://docs.zksync.io/build/developer-reference/differences-with-ethereum.html#mstore-mload
        // "Unlike EVM, where the memory growth is in words, on zkEVM the memory growth is counted in bytes."
        // "For example, if you write mstore(100, 0) the msize on zkEVM will be 132, but on the EVM it will be 160."
        let expected = U256::try_from(132).unwrap();
        let received = U256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
        assert_eq!(received, expected);
    }

    #[test]
    fn mstore8() {
        sol!(
            #[derive(Debug, PartialEq, Eq)]
            contract MStore8 {
                function mStore8(uint value) public pure returns (uint256 word);
            }
        );
        let code = crate::compile_blob("MStore8", include_str!("../contracts/mStore8.sol"));
        let (mut instance, export) = mock_runtime::prepare(&code, None);

        let mut assert = |parameter, expected| {
            let input = MStore8::mStore8Call::new((parameter,)).abi_encode();
            let state = crate::mock_runtime::call(State::new(input), &mut instance, export);

            assert_eq!(state.output.flags, 0);

            let received = U256::from_be_bytes::<32>(state.output.data.try_into().unwrap());
            assert_eq!(received, expected);
        };

        for (parameter, expected) in [
            (U256::MIN, U256::MIN),
            (
                U256::from(1),
                U256::from_str_radix(
                    "452312848583266388373324160190187140051835877600158453279131187530910662656",
                    10,
                )
                .unwrap(),
            ),
            (
                U256::from(2),
                U256::from_str_radix(
                    "904625697166532776746648320380374280103671755200316906558262375061821325312",
                    10,
                )
                .unwrap(),
            ),
            (
                U256::from(255),
                U256::from_str_radix(
                    "115339776388732929035197660848497720713218148788040405586178452820382218977280",
                    10,
                )
                .unwrap(),
            ),
            (
                U256::from(256),
                U256::from(0),
            ),
            (
                U256::from(257),
                U256::from_str_radix(
                    "452312848583266388373324160190187140051835877600158453279131187530910662656",
                    10,
                )
                .unwrap(),
            ),
            (
                U256::from(258),
                U256::from_str_radix(
                    "904625697166532776746648320380374280103671755200316906558262375061821325312",
                    10,
                )
                .unwrap(),
            ),
            (
                U256::from(123456789),
                U256::from_str_radix(
                    "9498569820248594155839807363993929941088553429603327518861754938149123915776",
                    10,
                )
                .unwrap(),
            ),
            (
                U256::MAX,
                U256::from_str_radix(
                    "115339776388732929035197660848497720713218148788040405586178452820382218977280",
                    10,
                )
                .unwrap(),
            ),
        ] {
            assert(parameter, expected);
        }
    }

    #[test]
    fn sha1() {
        let pre = vec![0xffu8; 512];
        let mut hasher = sha1::Sha1::new();
        hasher.update(&pre);
        let hash = hasher.finalize();

        let contract = Contract::sha1(pre);
        let (mut instance, export) = mock_runtime::prepare(&contract.pvm_runtime, None);
        let state = crate::mock_runtime::call(State::new(contract.calldata), &mut instance, export);
        assert_eq!(state.output.flags, 0);

        let expected = FixedBytes::<20>::from_slice(&hash[..]);
        let received = FixedBytes::<20>::from_slice(&state.output.data[..20]);
        assert_eq!(received, expected);
    }
}
