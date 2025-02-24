use alloy_primitives::{Address, Bytes, I256, U256};
use alloy_sol_types::{sol, SolCall, SolConstructor};

use revive_llvm_context::OptimizerSettings;
use revive_solidity::test_utils::*;

#[derive(Clone)]
pub struct Contract {
    pub name: &'static str,
    pub evm_runtime: Vec<u8>,
    pub pvm_runtime: Vec<u8>,
    pub calldata: Vec<u8>,
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
    }
);
case!("DivisionArithmetics.sol", DivisionArithmetics, divCall, division_arithmetics_div, n: U256, d: U256);
case!("DivisionArithmetics.sol", DivisionArithmetics, sdivCall, division_arithmetics_sdiv, n: I256, d: I256);
case!("DivisionArithmetics.sol", DivisionArithmetics, modCall, division_arithmetics_mod, n: U256, d: U256);
case!("DivisionArithmetics.sol", DivisionArithmetics, smodCall, division_arithmetics_smod, n: I256, d: I256);

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

        function CodeSize() public pure returns (uint ret);

        function ExtCodeHash(address who) public view returns (bytes32 ret);

        function CodeHash() public view returns (bytes32 ret);
    }
);
case!("ExtCode.sol", ExtCode, ExtCodeSizeCall, ext_code_size, address: Address);
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
    contract Storage {
        function transient(uint value) public returns (uint ret);
    }
);
case!("Storage.sol", Storage, transientCall, storage_transient, value: U256);

impl Contract {
    pub fn build(calldata: Vec<u8>, name: &'static str, code: &str) -> Self {
        Self {
            name,
            evm_runtime: compile_evm_bin_runtime(name, code),
            pvm_runtime: compile_blob(name, code),
            calldata,
        }
    }

    pub fn build_size_opt(calldata: Vec<u8>, name: &'static str, code: &str) -> Self {
        Self {
            name,
            evm_runtime: compile_evm_bin_runtime(name, code),
            pvm_runtime: compile_blob_with_options(name, code, true, OptimizerSettings::size()),
            calldata,
        }
    }
}

#[cfg(test)]
mod tests {
    use rayon::iter::{IntoParallelIterator, ParallelIterator};
    use serde::{de::Deserialize, Serialize};
    use std::{collections::BTreeMap, fs::File};

    use super::Contract;

    #[test]
    fn codesize() {
        let path = "codesize.json";

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
