use revive_runner::*;
use sha1::Digest;

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

/*
#[test]
fn balance() {
    // TODO: We do not have the correct balance API in the pallet yet
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
fn mstore8() {
    for (received, expected) in [
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
        (U256::from(256), U256::from(0)),
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
    ]
    .par_iter()
    .map(|(parameter, expected)| {
        let (_, output) = assert_success(&Contract::mstore8(*parameter), true);
        let received = U256::from_be_bytes::<32>(output.data.try_into().unwrap());
        (received, *expected)
    })
    .collect::<Vec<_>>()
    {
        assert_eq!(received, expected);
    }
}

#[test]
fn block_number() {
    let (_, output) = assert_success(&Contract::block_number(), true);
    let received = U256::from_be_bytes::<32>(output.data.try_into().unwrap());
    let expected = U256::from(mock_runtime::State::BLOCK_NUMBER);
    assert_eq!(received, expected);
}

#[test]
fn block_timestamp() {
    let (_, output) = assert_success(&Contract::block_timestamp(), true);
    let received = U256::from_be_bytes::<32>(output.data.try_into().unwrap());
    let expected = U256::from(mock_runtime::State::BLOCK_TIMESTAMP);
    assert_eq!(received, expected);
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
fn events() {
    assert_success(&Contract::event(U256::ZERO), true);
    assert_success(&Contract::event(U256::from(123)), true);
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

#[test]
fn mcopy() {
    let expected = vec![1, 2, 3];

    let (_, output) = assert_success(&Contract::memcpy(expected.clone()), false);

    let received = alloy_primitives::Bytes::abi_decode(&output.data, true)
        .unwrap()
        .to_vec();

    assert_eq!(expected, received);
}

#[test]
fn bitwise_byte() {
    assert_success(&Contract::bitwise_byte(U256::ZERO, U256::ZERO), true);
    assert_success(&Contract::bitwise_byte(U256::ZERO, U256::MAX), true);
    assert_success(&Contract::bitwise_byte(U256::MAX, U256::ZERO), true);
    assert_success(
        &Contract::bitwise_byte(U256::from_str("18446744073709551619").unwrap(), U256::MAX),
        true,
    );

    let de_bruijn_sequence =
        hex::decode("4060503824160d0784426150b864361d0f88c4a27148ac5a2f198d46e391d8f4").unwrap();
    let value = U256::from_be_bytes::<32>(de_bruijn_sequence.clone().try_into().unwrap());

    for (index, byte) in de_bruijn_sequence.iter().enumerate() {
        let (_, output) = assert_success(&Contract::bitwise_byte(U256::from(index), value), true);
        let expected = U256::from(*byte as i32);
        let received = U256::abi_decode(&output.data, true).unwrap();
        assert_eq!(expected, received)
    }
}

#[test]
fn transient_storage() {
    let expected = U256::MAX;
    let (state, output) = assert_success(&Contract::storage_transient(expected), false);
    let received = U256::abi_decode(&output.data, true).unwrap();
    assert_eq!(expected, received);

    assert!(state
        .accounts()
        .values()
        .all(|account| account.storage.is_empty()));
}
*/
