use std::str::FromStr;

use alloy_primitives::*;
use alloy_sol_types::SolCall;
use resolc::test_utils::build_yul;
use revive_runner::*;
use SpecsAction::*;

use crate::cases::Contract;
use crate::cases::DivisionArithmeticsConst;

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
test_spec!(revert_data_oob, "RevertDataOob", "RevertDataOob.sol");
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
test_spec!(erc7201, "ERC7201", "ERC7201.sol");
test_spec!(call_gas, "CallGas", "CallGas.sol");
test_spec!(linker_symbol, "Linked", "Linked.sol");
test_spec!(
    struct_delete_storage,
    "StructDeleteStorage",
    "StructDeleteStorage.sol"
);

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

/// Build calldata for the both-const Yul fixtures. Each fixture reads the case
/// index from `calldataload(0)` and a 32-byte "tag" from `calldataload(32)`
/// that gets XORed into the returned result. A non-zero tag turns silent
/// poison/undef from a UB-triggering const-fold into an observable divergence
/// from EVM (otherwise the buggy path coincidentally returns 0, which matches
/// EVM SMOD/SDIV's defined result for INT_MIN op -1 and masks the bug).
fn yul_which_calldata(which: u8) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[31] = which;
    // tag: 0xdeadbeef padded into the second 32-byte word
    data[60..64].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    data
}

fn instantiate_yul(path: &str, contract: &str) -> Vec<SpecsAction> {
    vec![Instantiate {
        origin: TestAddress::Alice,
        value: 0,
        gas_limit: Some(GAS_LIMIT),
        storage_deposit_limit: None,
        code: Code::Yul {
            path: path.into(),
            contract: contract.to_string(),
        },
        data: vec![],
        salt: OptionalHex::default(),
    }]
}

fn unsigned_const_set() -> (U256, U256, U256, U256) {
    (U256::from(1), U256::from(2), U256::from(5), U256::MAX)
}

fn signed_const_set() -> (I256, I256, I256, I256, I256, I256, I256, I256) {
    (
        I256::try_from(1).unwrap(),
        I256::try_from(2).unwrap(),
        I256::try_from(-2).unwrap(),
        I256::try_from(5).unwrap(),
        I256::try_from(-5).unwrap(),
        I256::MIN,
        I256::MIN + I256::ONE,
        I256::MAX,
    )
}

fn div_rhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if d == U256::ZERO {
        DivisionArithmeticsConst::divRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        DivisionArithmeticsConst::divRhsOneCall::new((n,)).abi_encode()
    } else if d == two {
        DivisionArithmeticsConst::divRhsTwoCall::new((n,)).abi_encode()
    } else if d == five {
        DivisionArithmeticsConst::divRhsFiveCall::new((n,)).abi_encode()
    } else if d == max {
        DivisionArithmeticsConst::divRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no divRhsConst variant for d={d}")
    }
}

fn div_lhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if n == U256::ZERO {
        DivisionArithmeticsConst::divLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        DivisionArithmeticsConst::divLhsOneCall::new((d,)).abi_encode()
    } else if n == two {
        DivisionArithmeticsConst::divLhsTwoCall::new((d,)).abi_encode()
    } else if n == five {
        DivisionArithmeticsConst::divLhsFiveCall::new((d,)).abi_encode()
    } else if n == max {
        DivisionArithmeticsConst::divLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no divLhsConst variant for n={n}")
    }
}

fn mod_rhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if d == U256::ZERO {
        DivisionArithmeticsConst::modRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        DivisionArithmeticsConst::modRhsOneCall::new((n,)).abi_encode()
    } else if d == two {
        DivisionArithmeticsConst::modRhsTwoCall::new((n,)).abi_encode()
    } else if d == five {
        DivisionArithmeticsConst::modRhsFiveCall::new((n,)).abi_encode()
    } else if d == max {
        DivisionArithmeticsConst::modRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no modRhsConst variant for d={d}")
    }
}

fn mod_lhs_const_data(n: U256, d: U256) -> Vec<u8> {
    let (one, two, five, max) = unsigned_const_set();
    if n == U256::ZERO {
        DivisionArithmeticsConst::modLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        DivisionArithmeticsConst::modLhsOneCall::new((d,)).abi_encode()
    } else if n == two {
        DivisionArithmeticsConst::modLhsTwoCall::new((d,)).abi_encode()
    } else if n == five {
        DivisionArithmeticsConst::modLhsFiveCall::new((d,)).abi_encode()
    } else if n == max {
        DivisionArithmeticsConst::modLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no modLhsConst variant for n={n}")
    }
}

fn sdiv_rhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, min_p1, max) = signed_const_set();
    if d == I256::ZERO {
        DivisionArithmeticsConst::sdivRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        DivisionArithmeticsConst::sdivRhsOneCall::new((n,)).abi_encode()
    } else if d == I256::MINUS_ONE {
        DivisionArithmeticsConst::sdivRhsNegOneCall::new((n,)).abi_encode()
    } else if d == two {
        DivisionArithmeticsConst::sdivRhsTwoCall::new((n,)).abi_encode()
    } else if d == neg_two {
        DivisionArithmeticsConst::sdivRhsNegTwoCall::new((n,)).abi_encode()
    } else if d == five {
        DivisionArithmeticsConst::sdivRhsFiveCall::new((n,)).abi_encode()
    } else if d == neg_five {
        DivisionArithmeticsConst::sdivRhsNegFiveCall::new((n,)).abi_encode()
    } else if d == min {
        DivisionArithmeticsConst::sdivRhsMinCall::new((n,)).abi_encode()
    } else if d == min_p1 {
        DivisionArithmeticsConst::sdivRhsMinPlusOneCall::new((n,)).abi_encode()
    } else if d == max {
        DivisionArithmeticsConst::sdivRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no sdivRhsConst variant for d={d}")
    }
}

fn sdiv_lhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, min_p1, max) = signed_const_set();
    if n == I256::ZERO {
        DivisionArithmeticsConst::sdivLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        DivisionArithmeticsConst::sdivLhsOneCall::new((d,)).abi_encode()
    } else if n == I256::MINUS_ONE {
        DivisionArithmeticsConst::sdivLhsNegOneCall::new((d,)).abi_encode()
    } else if n == two {
        DivisionArithmeticsConst::sdivLhsTwoCall::new((d,)).abi_encode()
    } else if n == neg_two {
        DivisionArithmeticsConst::sdivLhsNegTwoCall::new((d,)).abi_encode()
    } else if n == five {
        DivisionArithmeticsConst::sdivLhsFiveCall::new((d,)).abi_encode()
    } else if n == neg_five {
        DivisionArithmeticsConst::sdivLhsNegFiveCall::new((d,)).abi_encode()
    } else if n == min {
        DivisionArithmeticsConst::sdivLhsMinCall::new((d,)).abi_encode()
    } else if n == min_p1 {
        DivisionArithmeticsConst::sdivLhsMinPlusOneCall::new((d,)).abi_encode()
    } else if n == max {
        DivisionArithmeticsConst::sdivLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no sdivLhsConst variant for n={n}")
    }
}

fn smod_rhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, _min_p1, max) = signed_const_set();
    if d == I256::ZERO {
        DivisionArithmeticsConst::smodRhsZeroCall::new((n,)).abi_encode()
    } else if d == one {
        DivisionArithmeticsConst::smodRhsOneCall::new((n,)).abi_encode()
    } else if d == I256::MINUS_ONE {
        DivisionArithmeticsConst::smodRhsNegOneCall::new((n,)).abi_encode()
    } else if d == two {
        DivisionArithmeticsConst::smodRhsTwoCall::new((n,)).abi_encode()
    } else if d == neg_two {
        DivisionArithmeticsConst::smodRhsNegTwoCall::new((n,)).abi_encode()
    } else if d == five {
        DivisionArithmeticsConst::smodRhsFiveCall::new((n,)).abi_encode()
    } else if d == neg_five {
        DivisionArithmeticsConst::smodRhsNegFiveCall::new((n,)).abi_encode()
    } else if d == min {
        DivisionArithmeticsConst::smodRhsMinCall::new((n,)).abi_encode()
    } else if d == max {
        DivisionArithmeticsConst::smodRhsMaxCall::new((n,)).abi_encode()
    } else {
        panic!("no smodRhsConst variant for d={d}")
    }
}

fn smod_lhs_const_data(n: I256, d: I256) -> Vec<u8> {
    let (one, two, neg_two, five, neg_five, min, _min_p1, max) = signed_const_set();
    if n == I256::ZERO {
        DivisionArithmeticsConst::smodLhsZeroCall::new((d,)).abi_encode()
    } else if n == one {
        DivisionArithmeticsConst::smodLhsOneCall::new((d,)).abi_encode()
    } else if n == I256::MINUS_ONE {
        DivisionArithmeticsConst::smodLhsNegOneCall::new((d,)).abi_encode()
    } else if n == two {
        DivisionArithmeticsConst::smodLhsTwoCall::new((d,)).abi_encode()
    } else if n == neg_two {
        DivisionArithmeticsConst::smodLhsNegTwoCall::new((d,)).abi_encode()
    } else if n == five {
        DivisionArithmeticsConst::smodLhsFiveCall::new((d,)).abi_encode()
    } else if n == neg_five {
        DivisionArithmeticsConst::smodLhsNegFiveCall::new((d,)).abi_encode()
    } else if n == min {
        DivisionArithmeticsConst::smodLhsMinCall::new((d,)).abi_encode()
    } else if n == max {
        DivisionArithmeticsConst::smodLhsMaxCall::new((d,)).abi_encode()
    } else {
        panic!("no smodLhsConst variant for n={n}")
    }
}

fn push_call(actions: &mut Vec<SpecsAction>, dest: TestAddress, data: Vec<u8>) {
    actions.push(Call {
        origin: TestAddress::Alice,
        dest,
        value: 0,
        gas_limit: None,
        storage_deposit_limit: None,
        data,
    });
}

#[test]
fn unsigned_division_half_const() {
    let mut actions = instantiate(
        "contracts/DivisionArithmeticsConst.sol",
        "DivisionArithmeticsConst",
    );
    let (one, two, five, _max) = unsigned_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (five, two),
        (one, U256::ZERO),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            div_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            div_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn unsigned_division_both_const() {
    let mut actions = instantiate_yul("contracts/DivBothConst.yul", "DivBothConst");
    let pair_count = 5;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_division_half_const() {
    let mut actions = instantiate(
        "contracts/DivisionArithmeticsConst.sol",
        "DivisionArithmeticsConst",
    );
    let (one, two, neg_two, five, neg_five, _min, _min_p1, _max) = signed_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::ZERO, I256::MINUS_ONE),
        (five, two),
        (five, I256::MINUS_ONE),
        (I256::MINUS_ONE, neg_two),
        (neg_five, neg_five),
        (neg_five, two),
        (I256::MINUS_ONE, I256::MIN),
        (one, I256::ZERO),
        (I256::MIN, I256::MINUS_ONE),
        (I256::MIN + I256::ONE, I256::MINUS_ONE),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            sdiv_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            sdiv_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_division_both_const() {
    let mut actions = instantiate_yul("contracts/SdivBothConst.yul", "SdivBothConst");
    let pair_count = 13;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

#[test]
fn unsigned_remainder_half_const() {
    let mut actions = instantiate(
        "contracts/DivisionArithmeticsConst.sol",
        "DivisionArithmeticsConst",
    );
    let (one, two, five, _max) = unsigned_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (U256::ZERO, U256::MAX),
        (U256::MAX, U256::MAX),
        (five, two),
        (two, five),
        (U256::MAX, U256::ZERO),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            mod_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            mod_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn unsigned_remainder_both_const() {
    let mut actions = instantiate_yul("contracts/ModBothConst.yul", "ModBothConst");
    let pair_count = 7;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_remainder_half_const() {
    let mut actions = instantiate(
        "contracts/DivisionArithmeticsConst.sol",
        "DivisionArithmeticsConst",
    );
    let (one, two, neg_two, five, neg_five, _min, _min_p1, _max) = signed_const_set();
    let pairs = [
        (five, five),
        (five, one),
        (I256::ZERO, I256::MAX),
        (I256::MAX, I256::MAX),
        (five, two),
        (two, five),
        (five, neg_five),
        (five, I256::MINUS_ONE),
        (five, neg_two),
        (neg_five, two),
        (neg_two, five),
        (neg_five, neg_five),
        (neg_five, I256::MINUS_ONE),
        (neg_five, neg_two),
        (neg_two, neg_five),
        (I256::MIN, I256::MINUS_ONE),
        (I256::ZERO, I256::ZERO),
    ];
    for (n, d) in pairs {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            smod_rhs_const_data(n, d),
        );
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            smod_lhs_const_data(n, d),
        );
    }
    run_differential(actions);
}

#[test]
fn signed_remainder_both_const() {
    let mut actions = instantiate_yul("contracts/SmodBothConst.yul", "SmodBothConst");
    let pair_count = 17;
    for i in 0..pair_count {
        push_call(
            &mut actions,
            TestAddress::Instantiated(0),
            yul_which_calldata(i),
        );
    }
    run_differential(actions);
}

/// Surfaces the `smod(INT_MIN, -1)` LLVM-UB const-fold bug from
/// paritytech/revive#524. See `SmodIntMinNegOneBug.yul` for why this specific
/// fixture is shaped the way it is. Expected to FAIL until the bug is fixed.
#[test]
fn signed_remainder_int_min_neg_one_bug() {
    let mut actions = instantiate_yul("contracts/SmodIntMinNegOneBug.yul", "SmodIntMinNegOneBug");
    let mut tag = vec![0u8; 32];
    tag[28..32].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    push_call(&mut actions, TestAddress::Instantiated(0), tag);
    run_differential(actions);
}

/// Sibling to `signed_remainder_int_min_neg_one_bug`. Per the issue, sdiv is
/// claimed to be guarded; this test pins that guard so regressions surface.
#[test]
fn signed_division_int_min_neg_one_bug() {
    let mut actions = instantiate_yul("contracts/SdivIntMinNegOneBug.yul", "SdivIntMinNegOneBug");
    let mut tag = vec![0u8; 32];
    tag[28..32].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    push_call(&mut actions, TestAddress::Instantiated(0), tag);
    run_differential(actions);
}

/// Build the standard "tag plus extra calldata words" payload used by the
/// single-case probe fixtures.
fn probe_calldata(extra_words: &[U256]) -> Vec<u8> {
    let mut data = vec![0u8; 32 * (1 + extra_words.len())];
    data[28..32].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    for (i, word) in extra_words.iter().enumerate() {
        let off = 32 * (i + 1);
        data[off..off + 32].copy_from_slice(&word.to_be_bytes::<32>());
    }
    data
}

fn run_probe(path: &str, contract: &str, extra: &[U256]) {
    let mut actions = instantiate_yul(path, contract);
    push_call(
        &mut actions,
        TestAddress::Instantiated(0),
        probe_calldata(extra),
    );
    run_differential(actions);
}

#[test]
fn probe_shl_overflow() {
    run_probe(
        "contracts/ShlOverflowProbe.yul",
        "ShlOverflowProbe",
        &[U256::from(0x123456789abcdef_u64)],
    );
}

#[test]
fn probe_shr_overflow() {
    run_probe(
        "contracts/ShrOverflowProbe.yul",
        "ShrOverflowProbe",
        &[U256::from(0x123456789abcdef_u64)],
    );
}

#[test]
fn probe_sar_overflow() {
    run_probe("contracts/SarOverflowProbe.yul", "SarOverflowProbe", &[]);
}

#[test]
fn probe_addmod_zero() {
    run_probe(
        "contracts/AddModZeroProbe.yul",
        "AddModZeroProbe",
        &[U256::from(42), U256::from(99)],
    );
}

#[test]
fn probe_mulmod_zero() {
    run_probe(
        "contracts/MulModZeroProbe.yul",
        "MulModZeroProbe",
        &[U256::from(42), U256::from(99)],
    );
}

#[test]
fn probe_signextend_oob() {
    run_probe(
        "contracts/SignExtendOobProbe.yul",
        "SignExtendOobProbe",
        &[U256::MAX],
    );
}

#[test]
fn probe_byte_oob() {
    run_probe("contracts/ByteOobProbe.yul", "ByteOobProbe", &[U256::MAX]);
}

#[test]
fn probe_exp_zero_zero() {
    run_probe("contracts/ExpZeroZeroProbe.yul", "ExpZeroZeroProbe", &[]);
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
