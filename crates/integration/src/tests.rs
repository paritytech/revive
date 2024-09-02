use std::str::FromStr;

use alloy_primitives::*;
use revive_runner::*;
use SpecsAction::*;

use crate::cases::Contract;

macro_rules! test_spec {
    ($test_name:ident, $contract_name:literal, $source_file:literal) => {
        #[test]
        fn $test_name() {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("should always exist");
            let path = format!("{manifest_dir}/../integration/contracts/{}", $source_file);
            specs_from_comment($contract_name, &path).remove(0).run();
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
test_spec!(transferred_value, "Value", "Value.sol");
test_spec!(sha1, "SHA1", "SHA1.sol");
test_spec!(block, "Block", "Block.sol");
test_spec!(mcopy, "MCopy", "MCopy.sol");
test_spec!(events, "Events", "Events.sol");
test_spec!(storage, "Storage", "Storage.sol");
test_spec!(mstore8, "MStore8", "MStore8.sol");

#[test]
fn bitwise_byte() {
    let mut actions = vec![Instantiate {
        origin: TestAccountId::Alice,
        value: 0,
        gas_limit: Some(GAS_LIMIT),
        storage_deposit_limit: None,
        code: Code::Solidity {
            path: Some("contracts/Bitwise.sol".into()),
            contract: "Bitwise".to_string(),
            solc_optimizer: None,
            pipeline: None,
        },
        data: vec![],
        salt: vec![],
    }];

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
            origin: TestAccountId::Alice,
            dest: TestAccountId::Instantiated(0),
            value: 0,
            gas_limit: None,
            storage_deposit_limit: None,
            data: input,
        })
    }

    Specs {
        differential: true,
        balances: vec![(ALICE, 1_000_000_000)],
        actions,
    }
    .run();
}

/*
#[test]
fn events() {
    assert_success(&Contract::event(U256::ZERO), true);
    assert_success(&Contract::event(U256::from(123)), true);
}

#[test]
fn balance() {
    let (_, output) = assert_success(&Contract::value_balance_of(Default::default()), false);

    let expected = U256::ZERO;
    let received = U256::from_be_slice(&output.data);
    assert_eq!(expected, received);

    let expected = U256::from(54589);
    let (mut state, address) = State::new_deployed(Contract::value_balance_of(Default::default()));
    state.accounts_mut().get_mut(&address).unwrap().value = expected;

    let contract = Contract::value_balance_of(address);
    let (_, output) = state
        .transaction()
        .with_default_account(&contract.pvm_runtime)
        .calldata(contract.calldata)
        .call();

    assert_eq!(ReturnFlags::Success, output.flags);

    let received = U256::from_be_slice(&output.data);
    assert_eq!(expected, received)
}


#[test]
fn address() {
    let contract = Contract::context_address();
    let (_, output) = assert_success(&contract, true);
    let received = Address::from_slice(&output.data[12..]);
    let expected = Transaction::default_address();
    assert_eq!(received, expected);
}

#[test]
fn caller() {
    let (_, output) = assert_success(&Contract::context_caller(), true);
    let received = Address::from_slice(&output.data[12..]);
    let expected = Transaction::default_address();
    assert_eq!(received, expected);
}

#[test]
fn unsigned_division() {
    let one = U256::from(1);
    let two = U256::from(2);
    let five = U256::from(5);
    for (received, expected) in [
        (five, five, one),
        (five, one, five),
        (U256::ZERO, U256::MAX, U256::ZERO),
        (five, two, two),
        (one, U256::ZERO, U256::ZERO),
    ]
    .par_iter()
    .map(|(n, d, q)| {
        let (_, output) = assert_success(&Contract::division_arithmetics_div(*n, *d), true);
        let received = U256::from_be_bytes::<32>(output.data.try_into().unwrap());
        (received, *q)
    })
    .collect::<Vec<_>>()
    {
        assert_eq!(received, expected)
    }
}

#[test]
fn signed_division() {
    let one = I256::try_from(1).unwrap();
    let two = I256::try_from(2).unwrap();
    let minus_two = I256::try_from(-2).unwrap();
    let five = I256::try_from(5).unwrap();
    let minus_five = I256::try_from(-5).unwrap();
    for (received, expected) in [
        (five, five, one),
        (five, one, five),
        (I256::ZERO, I256::MAX, I256::ZERO),
        (I256::ZERO, I256::MINUS_ONE, I256::ZERO),
        (five, two, two),
        (five, I256::MINUS_ONE, minus_five),
        (I256::MINUS_ONE, minus_two, I256::ZERO),
        (minus_five, minus_five, one),
        (minus_five, two, minus_two),
        (I256::MINUS_ONE, I256::MIN, I256::ZERO),
        (one, I256::ZERO, I256::ZERO),
    ]
    .par_iter()
    .map(|(n, d, q)| {
        let (_, output) = assert_success(&Contract::division_arithmetics_sdiv(*n, *d), true);
        let received = I256::from_be_bytes::<32>(output.data.try_into().unwrap());
        (received, *q)
    })
    .collect::<Vec<_>>()
    {
        assert_eq!(received, expected);
    }
}

#[test]
fn unsigned_remainder() {
    let one = U256::from(1);
    let two = U256::from(2);
    let five = U256::from(5);
    for (received, expected) in [
        (five, five, U256::ZERO),
        (five, one, U256::ZERO),
        (U256::ZERO, U256::MAX, U256::ZERO),
        (U256::MAX, U256::MAX, U256::ZERO),
        (five, two, one),
        (two, five, two),
        (U256::MAX, U256::ZERO, U256::ZERO),
    ]
    .par_iter()
    .map(|(n, d, q)| {
        let (_, output) = assert_success(&Contract::division_arithmetics_mod(*n, *d), true);
        let received = U256::from_be_bytes::<32>(output.data.try_into().unwrap());
        (received, *q)
    })
    .collect::<Vec<_>>()
    {
        assert_eq!(received, expected);
    }
}

#[test]
fn signed_remainder() {
    let one = I256::try_from(1).unwrap();
    let two = I256::try_from(2).unwrap();
    let minus_two = I256::try_from(-2).unwrap();
    let five = I256::try_from(5).unwrap();
    let minus_five = I256::try_from(-5).unwrap();
    for (received, expected) in [
        (five, five, I256::ZERO),
        (five, one, I256::ZERO),
        (I256::ZERO, I256::MAX, I256::ZERO),
        (I256::MAX, I256::MAX, I256::ZERO),
        (five, two, one),
        (two, five, two),
        (five, minus_five, I256::ZERO),
        (five, I256::MINUS_ONE, I256::ZERO),
        (five, minus_two, one),
        (minus_five, two, I256::MINUS_ONE),
        (minus_two, five, minus_two),
        (minus_five, minus_five, I256::ZERO),
        (minus_five, I256::MINUS_ONE, I256::ZERO),
        (minus_five, minus_two, I256::MINUS_ONE),
        (minus_two, minus_five, minus_two),
        (I256::MIN, I256::MINUS_ONE, I256::ZERO),
        (I256::ZERO, I256::ZERO, I256::ZERO),
    ]
    .par_iter()
    .map(|(n, d, q)| {
        let (_, output) = assert_success(&Contract::division_arithmetics_smod(*n, *d), true);
        let received = I256::from_be_bytes::<32>(output.data.try_into().unwrap());
        (received, *q)
    })
    .collect::<Vec<_>>()
    {
        assert_eq!(received, expected);
    }
}

#[test]
fn create2() {
    let mut state = State::default();
    let contract_a = Contract::create_a();
    state.upload_code(&contract_a.pvm_runtime);

    let contract = Contract::create_b();
    let (state, output) = state
        .transaction()
        .with_default_account(&contract.pvm_runtime)
        .calldata(contract.calldata)
        .call();

    assert_eq!(output.flags, ReturnFlags::Success);
    assert_eq!(state.accounts().len(), 2);

    for address in state.accounts().keys() {
        if *address != Transaction::default_address() {
            let derived_address = Transaction::default_address().create2(
                B256::from(U256::from(1)),
                keccak256(&contract_a.pvm_runtime).0,
            );
            assert_eq!(*address, derived_address);
        }
    }
}

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

#[test]
fn create_with_value() {
    let mut state = State::default();
    state.upload_code(&Contract::create_a().pvm_runtime);
    let amount = U256::from(123);

    let contract = Contract::create_b();
    let (state, output) = state
        .transaction()
        .with_default_account(&contract.pvm_runtime)
        .callvalue(amount)
        .call();

    assert_eq!(output.flags, ReturnFlags::Success);
    assert_eq!(state.accounts().len(), 2);

    for (address, account) in state.accounts() {
        if *address == Transaction::default_address() {
            assert_eq!(account.value, U256::ZERO);
        } else {
            assert_eq!(account.value, amount);
        }
    }
}

#[test]
fn ext_code_size() {
    let contract = Contract::ext_code_size(Transaction::default_address());
    let (_, output) = assert_success(&contract, false);
    let received = U256::from_be_slice(&output.data);
    let expected = U256::from(contract.pvm_runtime.len());
    assert_eq!(received, expected);

    let contract = Contract::ext_code_size(Default::default());
    let (_, output) = assert_success(&contract, false);
    let received = U256::from_be_slice(&output.data);
    let expected = U256::ZERO;
    assert_eq!(received, expected);
}

#[test]
fn code_size() {
    let contract = Contract::code_size();
    let (_, output) = assert_success(&contract, false);
    let expected = U256::from(contract.pvm_runtime.len());
    let received = U256::from_be_slice(&output.data);
    assert_eq!(expected, received);
}

#[test]
fn value_transfer() {
    // Succeeds in remix (shanghai) but traps the interpreter
    let (state, _) = assert_success(&Contract::call_value_transfer(Default::default()), false);

    assert_eq!(state.accounts().len(), 2);
    assert!(state.accounts().get(&Address::default()).is_some());
}

#[test]
fn echo() {
    let (state, address) = State::new_deployed(Contract::call_constructor());

    let expected = vec![1, 2, 3, 4, 5];
    let contract = Contract::call_call(address, expected.clone());
    let (_, output) = state
        .transaction()
        .with_default_account(&contract.pvm_runtime)
        .calldata(contract.calldata)
        .call();

    assert_eq!(output.flags, ReturnFlags::Success);

    let received = alloy_primitives::Bytes::abi_decode(&output.data, true)
        .unwrap()
        .to_vec();

    assert_eq!(expected, received);
}
*/
