use std::str::FromStr;

use alloy_primitives::*;
use resolc::test_utils::build_yul;
use revive_runner::*;
use SpecsAction::*;

use crate::cases::Contract;

/// Parameters:
/// - The function name of the test
/// - The contract name to fill in empty code based on the file path
/// - The contract source file
macro_rules! test_spec {
    ($test_name:ident, $contract_name:literal, $source_file:literal) => {
        #[test]
        fn $test_name() {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("should always exist");
            let path = format!("{manifest_dir}/../integration/contracts/{}", $source_file);
            Specs::from_comment($contract_name, &path).remove(0).run();
        }
    };
}

test_spec!(baseline, "Baseline", "Baseline.sol");
test_spec!(flipper, "Flipper", "flipper.sol");
test_spec!(fibonacci_recursive, "FibonacciRecursive", "Fibonacci.sol");
test_spec!(fibonacci_iterative, "FibonacciIterative", "Fibonacci.sol");
test_spec!(fibonacci_binet, "FibonacciBinet", "Fibonacci.sol");
test_spec!(hash_keccak_256, "TestSha3", "Crypto.sol");
test_spec!(erc20, "ERC20", "ERC20.sol");
test_spec!(computation, "Computation", "Computation.sol");
test_spec!(msize, "MSize", "MSize.sol");
test_spec!(sha1, "SHA1", "SHA1.sol");
test_spec!(block, "Block", "Block.sol");
test_spec!(mcopy, "MCopy", "MCopy.sol");
test_spec!(mcopy_overlap, "MCopyOverlap", "MCopyOverlap.sol");
test_spec!(events, "Events", "Events.sol");
test_spec!(storage, "Storage", "Storage.sol");
test_spec!(mstore8, "MStore8", "MStore8.sol");
test_spec!(address, "Context", "Context.sol");
test_spec!(value, "Value", "Value.sol");
test_spec!(create, "CreateB", "Create.sol");
test_spec!(call, "Caller", "Call.sol");
test_spec!(balance, "Balance", "Balance.sol");
test_spec!(return_data_oob, "ReturnDataOob", "ReturnDataOob.sol");
test_spec!(immutables, "Immutables", "Immutables.sol");
test_spec!(transaction, "Transaction", "Transaction.sol");
test_spec!(block_hash, "BlockHash", "BlockHash.sol");
test_spec!(delegate, "Delegate", "Delegate.sol");
test_spec!(gas_price, "GasPrice", "GasPrice.sol");
test_spec!(gas_left, "GasLeft", "GasLeft.sol");
test_spec!(gas_limit, "GasLimit", "GasLimit.sol");
test_spec!(base_fee, "BaseFee", "BaseFee.sol");
test_spec!(coinbase, "Coinbase", "Coinbase.sol");
test_spec!(create2, "CreateB", "Create2.sol");
test_spec!(transfer, "Transfer", "Transfer.sol");
test_spec!(send, "Send", "Send.sol");
test_spec!(function_pointer, "FunctionPointer", "FunctionPointer.sol");
test_spec!(mload, "MLoad", "MLoad.sol");
test_spec!(delegate_no_contract, "DelegateCaller", "DelegateCaller.sol");
test_spec!(function_type, "FunctionType", "FunctionType.sol");
test_spec!(layout_at, "LayoutAt", "LayoutAt.sol");
test_spec!(shift_arithmetic_right, "SAR", "SAR.sol");
test_spec!(add_mod_mul_mod, "AddModMulModTester", "AddModMulMod.sol");
test_spec!(memory_bounds, "MemoryBounds", "MemoryBounds.sol");
test_spec!(selfdestruct, "Selfdestruct", "Selfdestruct.sol");
test_spec!(clz, "CountLeadingZeros", "CountLeadingZeros.sol");
test_spec!(call_gas, "CallGas", "CallGas.sol");
test_spec!(linker_symbol, "Linked", "Linked.sol");

fn instantiate(path: &str, contract: &str) -> Vec<SpecsAction> {
    vec![Instantiate {
        origin: TestAddress::Alice,
        value: 0,
        gas_limit: Some(GAS_LIMIT),
        storage_deposit_limit: None,
        code: Code::Solidity {
            path: Some(path.into()),
            contract: contract.to_string(),
            solc_optimizer: None,
            libraries: Default::default(),
        },
        data: vec![],
        salt: OptionalHex::default(),
    }]
}

fn run_differential(actions: Vec<SpecsAction>) {
    Specs {
        differential: true,
        actions,
        ..Default::default()
    }
    .run();
}

#[test]
fn bitwise_byte() {
    let mut actions = instantiate("contracts/Bitwise.sol", "Bitwise");

    let de_bruijn_sequence =
        hex::decode("4060503824160d0784426150b864361d0f88c4a27148ac5a2f198d46e391d8f4").unwrap();
    let value = U256::from_be_bytes::<32>(de_bruijn_sequence.clone().try_into().unwrap());
    for input in de_bruijn_sequence
        .iter()
        .enumerate()
        .map(|(index, _)| Contract::bitwise_byte(U256::from(index), value).calldata)
        .chain([
            Contract::bitwise_byte(U256::ZERO, U256::ZERO).calldata,
            Contract::bitwise_byte(U256::ZERO, U256::MAX).calldata,
            Contract::bitwise_byte(U256::MAX, U256::ZERO).calldata,
            Contract::bitwise_byte(U256::from_str("18446744073709551619").unwrap(), U256::MAX)
                .calldata,
        ])
    {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: input,
        })
    }

    run_differential(actions);
}

#[test]
fn unsigned_division() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = U256::from(1);
    let two = U256::from(2);
    let five = U256::from(5);
    for (n, d) in [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (five, two),
        (one, U256::ZERO),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_div(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn signed_division() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = I256::try_from(1).unwrap();
    let two = I256::try_from(2).unwrap();
    let minus_two = I256::try_from(-2).unwrap();
    let five = I256::try_from(5).unwrap();
    let minus_five = I256::try_from(-5).unwrap();
    for (n, d) in [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::ZERO, I256::MINUS_ONE),
        (five, two),
        (five, I256::MINUS_ONE),
        (I256::MINUS_ONE, minus_two),
        (minus_five, minus_five),
        (minus_five, two),
        (I256::MINUS_ONE, I256::MIN),
        (one, I256::ZERO),
        (I256::MIN, I256::MINUS_ONE),
        (I256::MIN + I256::ONE, I256::MINUS_ONE),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_sdiv(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn unsigned_remainder() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = U256::from(1);
    let two = U256::from(2);
    let five = U256::from(5);
    for (n, d) in [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (U256::MAX, U256::MAX),
        (five, two),
        (two, five),
        (U256::MAX, U256::ZERO),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_mod(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn signed_remainder() {
    let mut actions = instantiate("contracts/DivisionArithmetics.sol", "DivisionArithmetics");

    let one = I256::try_from(1).unwrap();
    let two = I256::try_from(2).unwrap();
    let minus_two = I256::try_from(-2).unwrap();
    let five = I256::try_from(5).unwrap();
    let minus_five = I256::try_from(-5).unwrap();
    for (n, d) in [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::MAX, I256::MAX),
        (five, two),
        (two, five),
        (five, minus_five),
        (five, I256::MINUS_ONE),
        (five, minus_two),
        (minus_five, two),
        (minus_two, five),
        (minus_five, minus_five),
        (minus_five, I256::MINUS_ONE),
        (minus_five, minus_two),
        (minus_two, minus_five),
        (I256::MIN, I256::MINUS_ONE),
        (I256::ZERO, I256::ZERO),
    ] {
        actions.push(Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: Contract::division_arithmetics_smod(n, d).calldata,
        })
    }

    run_differential(actions);
}

#[test]
fn ext_code_hash() {
    let mut actions = instantiate("contracts/ExtCode.sol", "ExtCode");

    // First do contract instantiation to figure out address and code hash
    let results = Specs {
        actions: actions.clone(),
        ..Default::default()
    }
    .run();
    let (addr, code_hash) = match results.first().cloned() {
        Some(CallResult::Instantiate {
            result, code_hash, ..
        }) => (result.result.unwrap().addr, code_hash),
        _ => panic!("instantiate contract failed"),
    };

    // code hash of itself
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::code_hash().calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from(code_hash.as_bytes().to_vec()),
        gas_consumed: None,
    }));

    // code hash for a given contract address
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::ext_code_hash(Address::from(addr.to_fixed_bytes())).calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from(code_hash.as_bytes().to_vec()),
        gas_consumed: None,
    }));

    // EOA returns fixed hash
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::ext_code_hash(Address::from(CHARLIE.to_fixed_bytes())).calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from(
            hex!("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470").to_vec(),
        ),
        gas_consumed: None,
    }));

    // non-existing account
    actions.push(Call {
        origin: TestAddress::Alice,
        dest: TestAddress::Instantiated(0),
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data: Contract::ext_code_hash(Address::from([8u8; 20])).calldata,
    });
    actions.push(VerifyCall(VerifyCallExpectation {
        success: true,
        output: OptionalHex::from([0u8; 32].to_vec()),
        gas_consumed: None,
    }));

    Specs {
        actions,
        ..Default::default()
    }
    .run();
}

#[test]
fn ext_code_size() {
    let alice = Address::from(ALICE.0);
    let own_address = alice.create(0);
    let baseline_address = alice.create2([0u8; 32], keccak256(Contract::baseline().pvm_runtime));

    let own_code_size = U256::from(
        Contract::ext_code_size(Default::default())
            .pvm_runtime
            .len(),
    );
    let baseline_code_size = U256::from(Contract::baseline().pvm_runtime.len());

    Specs {
        actions: vec![
            // Instantiate the test contract
            instantiate("contracts/ExtCode.sol", "ExtCode").remove(0),
            // Instantiate the baseline contract
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Solidity {
                    path: Some("contracts/Baseline.sol".into()),
                    contract: "Baseline".to_string(),
                    solc_optimizer: None,
                    libraries: Default::default(),
                },
                data: vec![],
                salt: OptionalHex::from([0; 32]),
            },
            // Alice is not a contract and returns a code size of 0
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(alice).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from([0u8; 32].to_vec()),
                gas_consumed: None,
            }),
            // Unknown address returns a code size of 0
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(Address::from([0xff; 20])).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from([0u8; 32].to_vec()),
                gas_consumed: None,
            }),
            // Own address via extcodesize returns own code size
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(own_address).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from(own_code_size.to_be_bytes::<32>().to_vec()),
                gas_consumed: None,
            }),
            // Own address via codesize returns own code size
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::code_size().calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from(own_code_size.to_be_bytes::<32>().to_vec()),
                gas_consumed: None,
            }),
            // Baseline address returns the baseline code size
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: 0,
                gas_limit: None,
                storage_deposit_limit: None,
                data: Contract::ext_code_size(baseline_address).calldata,
            },
            VerifyCall(VerifyCallExpectation {
                success: true,
                output: OptionalHex::from(baseline_code_size.to_be_bytes::<32>().to_vec()),
                gas_consumed: None,
            }),
        ],
        ..Default::default()
    }
    .run();
}

#[test]
fn create2_salt() {
    let salt = U256::from(777);
    let predicted = Contract::predicted_constructor(salt).pvm_runtime;
    let predictor = Contract::address_predictor_constructor(salt, predicted.clone().into());
    Specs {
        actions: vec![
            Upload {
                origin: TestAddress::Alice,
                code: Code::Bytes(predicted),
                storage_deposit_limit: None,
            },
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(predictor.pvm_runtime),
                data: predictor.calldata,
                salt: OptionalHex::default(),
            },
        ],
        differential: false,
        ..Default::default()
    }
    .run();
}

#[test]
fn code_block_stops() {
    let code = &build_yul(&[(
        "poc.yul",
        r#"object "Test"{
  code {
    tstore(0x7fd9d641,0x7b1e022)
    returndatacopy(0x0,0x0,returndatasize())
  }
  object "Test_deployed" { code{} }
}"#,
    )])
    .unwrap()["poc.yul:Test"];

    Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: Default::default(),
                gas_limit: None,
                storage_deposit_limit: None,
                data: Default::default(),
            },
            VerifyCall(Default::default()),
        ],
        differential: false,
        ..Default::default()
    }
    .run();
}

#[test]
fn code_block_with_nested_object_stops() {
    let code = &build_yul(&[(
        "poc.yul",
        r#"object "Test" {
    code {
        function allocate(size) -> ptr {
            ptr := mload(0x40)
            if iszero(ptr) { ptr := 0x60 }
            mstore(0x40, add(ptr, size))
        }
        let size := datasize("Test_deployed")
        let offset := allocate(size)
        datacopy(offset, dataoffset("Test_deployed"), size)
        return(offset, size)
    }
    object "Test_deployed" {
        code {
            sstore(0, 100)
	 }
        object "Test" {
            code {
	    revert(0,0)
            }
        }
    }
}"#,
    )])
    .unwrap()["poc.yul:Test"];

    Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            Call {
                origin: TestAddress::Alice,
                dest: TestAddress::Instantiated(0),
                value: Default::default(),
                gas_limit: None,
                storage_deposit_limit: None,
                data: Default::default(),
            },
            VerifyCall(Default::default()),
        ],
        differential: false,
        ..Default::default()
    }
    .run();
}

#[test]
fn sbrk_bounds_checks() {
    let code = &build_yul(&[(
        "poc.yul",
        r#"object "Test" {
    code {
        return(0x4, 0xffffffff)
        stop()
    }
    object "Test_deployed" {
        code {
            stop()
        }
    }
}"#,
    )])
    .unwrap()["poc.yul:Test"];

    let results = Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            VerifyCall(VerifyCallExpectation {
                success: false,
                ..Default::default()
            }),
        ],
        differential: false,
        ..Default::default()
    }
    .run();

    let CallResult::Instantiate { result, .. } = results.last().unwrap() else {
        unreachable!()
    };

    assert!(
        format!("{result:?}").contains("ContractTrapped"),
        "not seeing a trap means the contract did not catch the OOB"
    );
}

#[test]
fn invalid_opcode_works() {
    let code = &build_yul(&[(
        "invalid.yul",
        r#"object "Test" {
    code {
        invalid()
    }
    object "Test_deployed" {
        code {
            invalid()
        }
    }
}"#,
    )])
    .unwrap()["invalid.yul:Test"];

    let results = Specs {
        actions: vec![
            Instantiate {
                origin: TestAddress::Alice,
                value: 0,
                gas_limit: Some(GAS_LIMIT),
                storage_deposit_limit: None,
                code: Code::Bytes(code.to_vec()),
                data: Default::default(),
                salt: OptionalHex::default(),
            },
            VerifyCall(VerifyCallExpectation {
                success: false,
                ..Default::default()
            }),
        ],
        differential: false,
        ..Default::default()
    }
    .run();

    let CallResult::Instantiate { result, .. } = results.last().unwrap() else {
        unreachable!()
    };

    assert_eq!(result.weight_consumed, GAS_LIMIT);
}

/// Load from heap memory using an out of bounds offset and expect the
/// contract to hit the `invalid` syscall to use all gas (like on EVM).
///
/// The offset is picked such that a regular truncate would be in bounds.
#[test]
fn safe_truncate_int_to_xlen_works() {
    let offset = 0x10000000_00000000u64;
    let data = Contract::load_at(Uint::from(offset)).calldata;
    let mut actions = instantiate("contracts/MLoad.sol", "MLoad");
    actions.append(&mut vec![
        Call {
            origin: TestAddress::Alice,
            dest: TestAddress::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data,
        },
        VerifyCall(VerifyCallExpectation {
            success: false,
            ..Default::default()
        }),
    ]);

    let results = Specs {
        actions,
        differential: true,
        ..Default::default()
    }
    .run();

    let CallResult::Exec { result, .. } = results.last().unwrap() else {
        unreachable!()
    };

    assert_eq!(result.weight_consumed, GAS_LIMIT);
}
