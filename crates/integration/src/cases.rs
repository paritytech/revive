use alloy_primitives::{Address, Bytes, I256, U256};
use alloy_sol_types::{sol, SolCall, SolConstructor};

use resolc::test_utils::*;
use revive_llvm_context::OptimizerSettings;

#[derive(Clone)]
pub struct Contract {
    pub name: &'static str,
    pub evm_runtime: Vec<u8>,
    pub pvm_runtime: Vec<u8>,
    pub calldata: Vec<u8>,
    pub yul: String,
}

impl Contract {
    pub fn build(calldata: Vec<u8>, name: &'static str, code: &str) -> Self {
        Self {
            name,
            evm_runtime: compile_evm_bin_runtime(name, code, Default::default()),
            pvm_runtime: compile_blob(name, code),
            calldata,
            yul: compile_to_yul(name, code, true),
        }
    }

    pub fn build_size_opt(calldata: Vec<u8>, name: &'static str, code: &str) -> Self {
        Self {
            name,
            evm_runtime: compile_evm_bin_runtime(name, code, Default::default()),
            pvm_runtime: compile_blob_with_options(
                name,
                code,
                true,
                OptimizerSettings::size(),
                Default::default(),
            ),
            calldata,
            yul: compile_to_yul(name, code, true),
        }
    }
}

macro_rules! case {
    // Arguments:
    //     1. The file name, expect to live under "../contracts/"
    //     2. The Solidity contract name
    //     3. The derived Solidity function call name
    //     4. The method name on [Contract]
    //     5. Any parameters to the Solidity functions
    ($file_name:literal, $contract_name:ident, $contract_method:ident, $method_name:ident, $( $v:ident: $t:ty ),* ) => {
        impl Contract {
            pub fn $method_name($($v: $t),*) -> Self {
                let code = include_str!(concat!("../contracts/", $file_name));
                let args = $contract_name::$contract_method::new(($($v,)*)).abi_encode();
                let name = stringify!($contract_name);
                Contract::build(args, name, code)
            }
        }
    };

    // Arguments:
    //     1. The file name, expect to live under "../contracts/"
    //     2. The Solidity contract name
    //     3. Raw Calldata
    //     4. The method name on [Contract]
    ($file_name:literal, $contract_name:literal, $calldata:expr, $method_name:ident) => {
        impl Contract {
            pub fn $method_name() -> Self {
                let code = include_str!(concat!("../contracts/", $file_name));
                Contract::build($calldata, $contract_name, code)
            }
        }
    };
}

case!("Create.sol", "CreateA", vec![0; 4], create_a);
case!("Create.sol", "CreateB", vec![0; 4], create_b);

sol!(contract Baseline { function baseline() public payable; });
case!("Baseline.sol", Baseline, baselineCall, baseline,);

sol!(contract Flipper {
    constructor (bool);

    function flip() public;
});
case!("flipper.sol", Flipper, flipCall, flipper,);
case!("flipper.sol", Flipper, constructorCall, flipper_constructor, coin: bool);

sol!(contract Computation {
    function odd_product(int32 n) public pure returns (int64);
    function triangle_number(int64 n) public pure returns (int64 sum);
});
case!("Computation.sol", Computation, odd_productCall, odd_product, n: i32);
case!("Computation.sol", Computation, triangle_numberCall, triangle_number, n: i64);

sol!(
    contract FibonacciRecursive {
        function fib3(uint n) public pure returns (uint);
    }
);
case!("Fibonacci.sol", FibonacciRecursive, fib3Call, fib_recursive, n: U256);

sol!(
    contract FibonacciIterative {
        function fib3(uint n) external pure returns (uint b);
    }
);
case!("Fibonacci.sol", FibonacciIterative, fib3Call, fib_iterative, n: U256);

sol!(
    contract FibonacciBinet {
        function fib3(uint n) external pure returns (uint a);
    }
);
case!("Fibonacci.sol", FibonacciBinet, fib3Call, fib_binet, n: U256);

sol!(
    contract SHA1 {
        function sha1(bytes memory data) public pure returns (bytes20 ret);
    }
);
case!("SHA1.sol", SHA1, sha1Call, sha1, pre: Bytes);

sol!(
    contract ERC20 {
        function totalSupply() external view returns (uint);

        function balanceOf(address account) external view returns (uint);

        function transfer(address recipient, uint amount) external returns (bool);

        function allowance(
            address owner,
            address spender
        ) external view returns (uint);

        function approve(address spender, uint amount) external returns (bool);

        function transferFrom(
            address sender,
            address recipient,
            uint amount
        ) external returns (bool);

        event Transfer(address indexed from, address indexed to, uint value);
        event Approval(address indexed owner, address indexed spender, uint value);
    }
);
case!("ERC20.sol", ERC20, totalSupplyCall, erc20,);

sol!(
    contract Block {
        function timestamp() public view returns (uint ret);

        function number() public view returns (uint ret);
    }
);
case!("Block.sol", Block, numberCall, block_number,);
case!("Block.sol", Block, timestampCall, block_timestamp,);

sol!(
    contract Context {
        function address_this() public view returns (address);

        function caller() public pure returns (address);
    }
);
case!("Context.sol", Context, address_thisCall, context_address,);
case!("Context.sol", Context, callerCall, context_caller,);

sol!(
    contract DivisionArithmetics {
        function div(uint n, uint d) public pure returns (uint q);

        function sdiv(int n, int d) public pure returns (int q);

        function mod(uint n, uint d) public pure returns (uint r);

        function smod(int n, int d) public pure returns (int r);

        function divSelf(uint256 x) external pure returns (uint256 r);
        function sdivSelf(int256 x) external pure returns (int256 r);
        function modSelf(uint256 x) external pure returns (uint256 r);
    }
);
case!("DivisionArithmetics.sol", DivisionArithmetics, divCall, division_arithmetics_div, n: U256, d: U256);
case!("DivisionArithmetics.sol", DivisionArithmetics, sdivCall, division_arithmetics_sdiv, n: I256, d: I256);
case!("DivisionArithmetics.sol", DivisionArithmetics, modCall, division_arithmetics_mod, n: U256, d: U256);
case!("DivisionArithmetics.sol", DivisionArithmetics, smodCall, division_arithmetics_smod, n: I256, d: I256);
case!("DivisionArithmetics.sol", DivisionArithmetics, divSelfCall, division_arithmetics_div_self, x: U256);
case!("DivisionArithmetics.sol", DivisionArithmetics, sdivSelfCall, division_arithmetics_sdiv_self, x: I256);
case!("DivisionArithmetics.sol", DivisionArithmetics, modSelfCall, division_arithmetics_mod_self, x: U256);

sol!(
    contract SDivNarrowBug {
        function sdiv_masked(uint256 a, uint256 b) external pure returns (int256 q);
    }
);
case!("SDivNarrowBug.sol", SDivNarrowBug, sdiv_maskedCall, sdiv_narrow_bug_masked, a: U256, b: U256);

sol!(
    contract KeccakFuseBug {
        function probe(uint256[8] calldata seeds) external view returns (uint256 r, bytes32 sink_out);
    }
);
case!(
    "KeccakFuseBug.sol",
    KeccakFuseBug,
    probeCall,
    keccak_fuse_bug_probe,
    seeds: [U256; 8]
);

sol!(
    contract ParamMload {
        function tryFetch(uint256 x) external pure returns (uint256);
    }
);
case!("ParamMload.sol", ParamMload, tryFetchCall, param_mload_try_fetch, x: U256);

// `PanicCodeBug` has only a `fallback()` — invoked by sending raw
// calldata that doesn't match any function selector. Empty calldata
// suffices.
case!(
    "PanicCodeBug.sol",
    "PanicCodeBug",
    vec![],
    panic_code_bug_trigger
);

// `FmpDynStoreBug` is invoked via empty fallback, passing 0x40 as the
// only calldata word so the inline-asm `calldataload(0)` resolves to
// 0x40 at runtime but stays opaque to the simplifier.
case!(
    "FmpDynStoreBug.sol",
    "FmpDynStoreBug",
    {
        let mut bytes = vec![0u8; 32];
        bytes[31] = 0x40;
        bytes
    },
    fmp_dyn_store_bug
);

// `FmpRangeProofBug` reads the 32-byte calldata as the value to put
// into the FMP slot. Use `0x100000007 = 2^32 + 7` (well beyond the
// 17-bit heap-size range) to make the range-proof truncation visible.
case!(
    "FmpRangeProofBug.sol",
    "FmpRangeProofBug",
    {
        let mut bytes = vec![0u8; 32];
        bytes[31] = 0x07;
        bytes[27] = 0x01;
        bytes
    },
    fmp_range_proof_bug
);

// `FmpNativeStoreBug` calldata: first word = FMP value `0x100000007`,
// second word = non-zero condition to force the if-branch.
case!(
    "FmpNativeStoreBug.sol",
    "FmpNativeStoreBug",
    {
        let mut bytes = vec![0u8; 64];
        bytes[31] = 0x07;
        bytes[27] = 0x01;
        bytes[63] = 0x01;
        bytes
    },
    fmp_native_store_bug
);

// `FmpCrossObjectBug` calldata: first word = FMP value `0x100000007`,
// second word = recursion depth (0 to hit the inner inline-asm branch).
case!(
    "FmpCrossObjectBug.sol",
    "FmpCrossObjectBug",
    {
        let mut bytes = vec![0u8; 64];
        bytes[31] = 0x07;
        bytes[27] = 0x01;
        bytes
    },
    fmp_cross_object_bug
);

// `FmpRevertBug` is invoked via empty calldata into its `fallback()`.
case!("FmpRevertBug.sol", "FmpRevertBug", vec![], fmp_revert_bug);

// `FmpDynRevertBug` reads `offset` and `length` from calldata: pass
// 64 bytes encoding `offset=0`, `length=96` so the dynamic revert
// covers the FMP slot.
case!(
    "FmpDynRevertBug.sol",
    "FmpDynRevertBug",
    {
        let mut bytes = vec![0u8; 64];
        bytes[63] = 96;
        bytes
    },
    fmp_dyn_revert_bug
);

sol!(
    contract UnalignedMStoreBug {
        function bug() external pure returns (bytes32);
    }
);
case!(
    "UnalignedMStoreBug.sol",
    UnalignedMStoreBug,
    bugCall,
    unaligned_mstore_bug,
);

sol!(
    contract ConstReturnOverflowBug {
        function bug() external pure returns (uint256);
    }
);
case!(
    "ConstReturnOverflowBug.sol",
    ConstReturnOverflowBug,
    bugCall,
    const_return_overflow_bug,
);

sol!(
    contract LinkerI32BoundaryFoldBug {
        function test(int256 a0, int256 a2) external pure returns (int256);
    }
);
case!(
    "LinkerI32BoundaryFoldBug.sol",
    LinkerI32BoundaryFoldBug,
    testCall,
    linker_i32_boundary_fold_bug,
    a0: I256,
    a2: I256
);

// `PanicInterveneBug` has only a `fallback()` — invoked by sending
// raw calldata that doesn't match any selector. Empty calldata works.
case!(
    "PanicInterveneBug.sol",
    "PanicInterveneBug",
    vec![],
    panic_intervene_bug
);

sol!(
    contract UnalignedMStore8Bug {
        function bug() external pure returns (bytes32);
    }
);
case!(
    "UnalignedMStore8Bug.sol",
    UnalignedMStore8Bug,
    bugCall,
    unaligned_mstore8_bug,
);

sol!(
    contract CopyOverlapBug {
        function bug(uint256 length) external pure returns (bytes32);
    }
);
case!(
    "CopyOverlapBug.sol",
    CopyOverlapBug,
    bugCall,
    copy_overlap_bug,
    length: U256
);

sol!(
    contract CallerOriginAliasing {
        function caller_then_origin() external view returns (address, address);
        function origin_then_caller() external view returns (address, address);
        function caller_address_origin() external view returns (address, address, address);
        function repeated_caller() external view returns (address, address, address);
    }
);
case!(
    "CallerOriginAliasing.sol",
    CallerOriginAliasing,
    caller_then_originCall,
    caller_origin_aliasing_caller_then_origin,
);
case!(
    "CallerOriginAliasing.sol",
    CallerOriginAliasing,
    origin_then_callerCall,
    caller_origin_aliasing_origin_then_caller,
);
case!(
    "CallerOriginAliasing.sol",
    CallerOriginAliasing,
    caller_address_originCall,
    caller_origin_aliasing_caller_address_origin,
);
case!(
    "CallerOriginAliasing.sol",
    CallerOriginAliasing,
    repeated_callerCall,
    caller_origin_aliasing_repeated_caller,
);

sol!(
    contract DivConst {
        function divRhsZero(uint256 n) external pure returns (uint256);
        function divRhsOne(uint256 n) external pure returns (uint256);
        function divRhsTwo(uint256 n) external pure returns (uint256);
        function divRhsFive(uint256 n) external pure returns (uint256);
        function divRhsMax(uint256 n) external pure returns (uint256);
        function divLhsZero(uint256 d) external pure returns (uint256);
        function divLhsOne(uint256 d) external pure returns (uint256);
        function divLhsTwo(uint256 d) external pure returns (uint256);
        function divLhsFive(uint256 d) external pure returns (uint256);
        function divLhsMax(uint256 d) external pure returns (uint256);
    }

    contract SdivConst {
        function sdivRhsZero(int256 n) external pure returns (int256);
        function sdivRhsOne(int256 n) external pure returns (int256);
        function sdivRhsNegOne(int256 n) external pure returns (int256);
        function sdivRhsTwo(int256 n) external pure returns (int256);
        function sdivRhsNegTwo(int256 n) external pure returns (int256);
        function sdivRhsFive(int256 n) external pure returns (int256);
        function sdivRhsNegFive(int256 n) external pure returns (int256);
        function sdivRhsMin(int256 n) external pure returns (int256);
        function sdivRhsMinPlusOne(int256 n) external pure returns (int256);
        function sdivRhsMax(int256 n) external pure returns (int256);
        function sdivLhsZero(int256 d) external pure returns (int256);
        function sdivLhsOne(int256 d) external pure returns (int256);
        function sdivLhsNegOne(int256 d) external pure returns (int256);
        function sdivLhsTwo(int256 d) external pure returns (int256);
        function sdivLhsNegTwo(int256 d) external pure returns (int256);
        function sdivLhsFive(int256 d) external pure returns (int256);
        function sdivLhsNegFive(int256 d) external pure returns (int256);
        function sdivLhsMin(int256 d) external pure returns (int256);
        function sdivLhsMinPlusOne(int256 d) external pure returns (int256);
        function sdivLhsMax(int256 d) external pure returns (int256);
    }

    contract ModConst {
        function modRhsZero(uint256 n) external pure returns (uint256);
        function modRhsOne(uint256 n) external pure returns (uint256);
        function modRhsTwo(uint256 n) external pure returns (uint256);
        function modRhsFive(uint256 n) external pure returns (uint256);
        function modRhsMax(uint256 n) external pure returns (uint256);
        function modLhsZero(uint256 d) external pure returns (uint256);
        function modLhsOne(uint256 d) external pure returns (uint256);
        function modLhsTwo(uint256 d) external pure returns (uint256);
        function modLhsFive(uint256 d) external pure returns (uint256);
        function modLhsMax(uint256 d) external pure returns (uint256);
    }

    contract SmodConst {
        function smodRhsZero(int256 n) external pure returns (int256);
        function smodRhsOne(int256 n) external pure returns (int256);
        function smodRhsNegOne(int256 n) external pure returns (int256);
        function smodRhsTwo(int256 n) external pure returns (int256);
        function smodRhsNegTwo(int256 n) external pure returns (int256);
        function smodRhsFive(int256 n) external pure returns (int256);
        function smodRhsNegFive(int256 n) external pure returns (int256);
        function smodRhsMin(int256 n) external pure returns (int256);
        function smodRhsMax(int256 n) external pure returns (int256);
        function smodLhsZero(int256 d) external pure returns (int256);
        function smodLhsOne(int256 d) external pure returns (int256);
        function smodLhsNegOne(int256 d) external pure returns (int256);
        function smodLhsTwo(int256 d) external pure returns (int256);
        function smodLhsNegTwo(int256 d) external pure returns (int256);
        function smodLhsFive(int256 d) external pure returns (int256);
        function smodLhsNegFive(int256 d) external pure returns (int256);
        function smodLhsMin(int256 d) external pure returns (int256);
        function smodLhsMax(int256 d) external pure returns (int256);
    }
);

sol!(
    contract Send {
        function transfer_self(uint _amount) public payable;
    }
);
case!("Send.sol", Send, transfer_selfCall, send_self, amount: U256);

sol!(
    contract Transfer {
        function transfer_self(uint _amount) public payable;
    }
);
case!("Transfer.sol", Transfer, transfer_selfCall, transfer_self, amount: U256);

sol!(
    contract MStore8 {
        function mStore8(uint value) public pure returns (uint256 word);
    }
);
case!("MStore8.sol", MStore8, mStore8Call, mstore8, value: U256);

sol!(
    contract Events {
        event A(uint) anonymous;
        event E(uint indexed, uint indexed, uint indexed);

        function emitEvent(uint topics) public;
    }
);
case!("Events.sol", Events, emitEventCall, event, topics: U256);

sol!(
    contract ExtCode {
        function ExtCodeSize(address who) public view returns (uint ret);

        function ExtCodeSizeSum(address a, address b) public view returns (uint ret);

        function CodeSize() public pure returns (uint ret);

        function ExtCodeHash(address who) public view returns (bytes32 ret);

        function CodeHash() public view returns (bytes32 ret);
    }
);
case!("ExtCode.sol", ExtCode, ExtCodeSizeCall, ext_code_size, address: Address);
case!("ExtCode.sol", ExtCode, ExtCodeSizeSumCall, ext_code_size_sum, a: Address, b: Address);
case!("ExtCode.sol", ExtCode, CodeSizeCall, code_size,);
case!("ExtCode.sol", ExtCode, ExtCodeHashCall, ext_code_hash, address: Address);
case!("ExtCode.sol", ExtCode, CodeHashCall, code_hash,);

sol!(
    contract MCopy {
        function memcpy(bytes memory payload) public pure returns (bytes memory);
    }
);
case!("MCopy.sol", MCopy, memcpyCall, memcpy, payload: Bytes);

sol!(
    contract MLoad {
        constructor() payable;

        function loadAt(uint _offset) public payable returns (uint m);
    }
);
case!("MLoad.sol", MLoad, loadAtCall, load_at, _offset: U256);

sol!(
    contract Call {
        function value_transfer(address payable destination) public payable;

        function echo(bytes memory payload) public payable returns (bytes memory);

        function call(
            address callee,
            bytes memory payload
        ) public payable returns (bytes memory);
    }
);
case!("Call.sol", Call, value_transferCall, call_value_transfer, destination: Address);
case!("Call.sol", Call, callCall, call_call, destination: Address, payload: Bytes);
case!("Call.sol", "Call", vec![], call_constructor);

sol!(
    contract Value {
        function balance_of(address _address) public view returns (uint ret);
        function balance_self() public view returns (uint ret);
    }
);
case!("Value.sol", Value, balance_ofCall, value_balance_of, address: Address);
case!("Value.sol", Value, balance_selfCall, value_balance_self,);

sol!(
    contract Bitwise {
        function opByte(uint i, uint x) public payable returns (uint ret);
    }
);
case!("Bitwise.sol", Bitwise, opByteCall, bitwise_byte, index: U256, value: U256);

sol!(
    contract UlongRem {
        function bigMulMod(uint a, uint b, uint m) external pure returns (uint);
    }
);
case!("UlongRem.sol", UlongRem, bigMulModCall, ulongrem_big_mulmod, a: U256, b: U256, m: U256);

sol!(
    contract Storage {
        function transient(uint value) public returns (uint ret);
    }
);
case!("Storage.sol", Storage, transientCall, storage_transient, value: U256);

sol!(
    contract Predicted {
        constructor(uint _foo);
    }
    contract AddressPredictor {
        constructor(uint _foo, bytes memory _bytecode) payable;
    }
);
case!("AddressPredictor.sol", Predicted, constructorCall, predicted_constructor, salt: U256);
case!("AddressPredictor.sol", AddressPredictor, constructorCall, address_predictor_constructor, salt: U256, bytecode: Bytes);

#[cfg(test)]
mod tests {
    use rayon::iter::{IntoParallelIterator, ParallelIterator};
    use serde::{de::Deserialize, Serialize};
    use std::{collections::BTreeMap, fs::File};

    use super::Contract;

    #[test]
    fn codesize() {
        let path = if cfg!(feature = "newyork") {
            "codesize_newyork.json"
        } else {
            "codesize.json"
        };

        let existing = File::open(path)
            .map(|file| {
                BTreeMap::<String, usize>::deserialize(&mut serde_json::Deserializer::from_reader(
                    file,
                ))
                .expect("should be able to deserialze codesize data")
            })
            .ok();

        let extract_code_size = |compile: fn() -> Contract| {
            let contract = compile();
            let contract_length = contract.pvm_runtime.len();
            let size_change = existing
                .as_ref()
                .and_then(|map| map.get(contract.name))
                .filter(|old| **old != 0)
                .map(|old| {
                    let old = *old as f32;
                    let p = (contract_length as f32 - old) / old * 100.0;
                    format!("({p}% change from {old} bytes)",)
                })
                .unwrap_or_default();

            println!("{}: {contract_length} bytes {size_change}", contract.name);

            (contract.name, contract_length)
        };

        [
            (|| {
                Contract::build_size_opt(
                    vec![],
                    "Baseline",
                    include_str!("../contracts/Baseline.sol"),
                )
            }) as _,
            (|| {
                Contract::build_size_opt(
                    vec![],
                    "Flipper",
                    include_str!("../contracts/flipper.sol"),
                )
            }) as _,
            (|| {
                Contract::build_size_opt(
                    vec![],
                    "Computation",
                    include_str!("../contracts/Computation.sol"),
                )
            }) as _,
            (|| {
                Contract::build_size_opt(
                    vec![],
                    "FibonacciIterative",
                    include_str!("../contracts/Fibonacci.sol"),
                )
            }) as _,
            (|| Contract::build_size_opt(vec![], "ERC20", include_str!("../contracts/ERC20.sol")))
                as _,
            (|| Contract::build_size_opt(vec![], "SHA1", include_str!("../contracts/SHA1.sol")))
                as _,
            (|| {
                Contract::build_size_opt(
                    vec![],
                    "DivisionArithmetics",
                    include_str!("../contracts/DivisionArithmetics.sol"),
                )
            }) as _,
            (|| Contract::build_size_opt(vec![], "Events", include_str!("../contracts/Events.sol")))
                as _,
        ]
        .into_par_iter()
        .map(extract_code_size)
        .collect::<BTreeMap<_, _>>()
        .serialize(&mut serde_json::Serializer::pretty(
            File::create(path).unwrap(),
        ))
        .unwrap_or_else(|err| panic!("can not write codesize data to '{path}': {err}"));
    }
}
