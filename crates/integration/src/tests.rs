use std::str::FromStr;

use alloy_primitives::*;
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
test_spec!(events, "Events", "Events.sol");
test_spec!(storage, "Storage", "Storage.sol");
test_spec!(mstore8, "MStore8", "MStore8.sol");
test_spec!(address, "Context", "Context.sol");
test_spec!(balance, "Value", "Value.sol");
test_spec!(create, "CreateB", "Create.sol");
test_spec!(call, "Caller", "Call.sol");
test_spec!(transfer, "Transfer", "Transfer.sol");
test_spec!(return_data_oob, "ReturnDataOob", "ReturnDataOob.sol");
test_spec!(immutables, "Immutables", "Immutables.sol");
test_spec!(transaction, "Transaction", "Transaction.sol");
test_spec!(block_hash, "BlockHash", "BlockHash.sol");
test_spec!(delegate, "Delegate", "Delegate.sol");
test_spec!(gas_price, "GasPrice", "GasPrice.sol");

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
            pipeline: None,
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
                    pipeline: None,
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

/*
// These test were implement for the mock-runtime and need to be ported yet.

#[test]
fn create2_failure() {
    let mut state = State::default();
    let contract_a = Contract::create_a();
    state.upload_code(&contract_a.pvm_runtime);

    let contract = Contract::create_b();
    let (state, output) = state
        .transaction()
        .with_default_account(&contract.pvm_runtime)
        .calldata(contract.calldata.clone())
        .call();

    assert_eq!(output.flags, ReturnFlags::Success);

    // The address already exists, which should cause the contract to revert

    let (_, output) = state
        .transaction()
        .with_default_account(&contract.pvm_runtime)
        .calldata(contract.calldata)
        .call();

    assert_eq!(output.flags, ReturnFlags::Revert);
}
*/
