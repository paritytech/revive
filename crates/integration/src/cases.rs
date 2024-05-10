use alloy_primitives::{I256, U256};
use alloy_sol_types::{sol, SolCall};

#[derive(Clone)]
pub struct Contract {
    pub name: &'static str,
    pub evm_runtime: Vec<u8>,
    pub pvm_runtime: Vec<u8>,
    pub calldata: Vec<u8>,
}

sol!(contract Baseline { function baseline() public payable; });

sol!(contract Flipper { function flip() public; });

sol!(contract Computation {
    function odd_product(int32 n) public pure returns (int64);
    function triangle_number(int64 n) public pure returns (int64 sum);
});

sol!(
    contract FibonacciRecursive {
        function fib3(uint n) public pure returns (uint);
    }
);

sol!(
    contract FibonacciIterative {
        function fib3(uint n) external pure returns (uint b);
    }
);

sol!(
    contract FibonacciBinet {
        function fib3(uint n) external pure returns (uint a);
    }
);

sol!(
    contract SHA1 {
        function sha1(bytes memory data) public pure returns (bytes20 ret);
    }
);

sol!(
    interface IERC20 {
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

sol!(
    contract Block {
        function timestamp() public view returns (uint ret);

        function number() public view returns (uint ret);
    }
);

sol!(
    contract Context {
        function address_this() public view returns (address);

        function caller() public pure returns (address);
    }
);

sol!(
    contract DivisionArithmetics {
        function div(uint n, uint d) public pure returns (uint q);

        function sdiv(int n, int d) public pure returns (int q);

        function mod(uint n, uint d) public pure returns (uint r);

        function smod(int n, int d) public pure returns (int r);
    }
);

sol!(
    contract MStore8 {
        function mStore8(uint value) public pure returns (uint256 word);
    }
);

impl Contract {
    pub fn baseline() -> Self {
        let code = include_str!("../contracts/Baseline.sol");
        let name = "Baseline";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Baseline::baselineCall::new(()).abi_encode(),
        }
    }

    pub fn odd_product(n: i32) -> Self {
        let code = include_str!("../contracts/Computation.sol");
        let name = "Computation";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Computation::odd_productCall::new((n,)).abi_encode(),
        }
    }

    pub fn triangle_number(n: i64) -> Self {
        let code = include_str!("../contracts/Computation.sol");
        let name = "Computation";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Computation::triangle_numberCall::new((n,)).abi_encode(),
        }
    }

    pub fn fib_recursive(n: u32) -> Self {
        let code = include_str!("../contracts/Fibonacci.sol");
        let name = "FibonacciRecursive";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: FibonacciRecursive::fib3Call::new((U256::from(n),)).abi_encode(),
        }
    }

    pub fn fib_iterative(n: u32) -> Self {
        let code = include_str!("../contracts/Fibonacci.sol");
        let name = "FibonacciIterative";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: FibonacciIterative::fib3Call::new((U256::from(n),)).abi_encode(),
        }
    }

    pub fn fib_binet(n: u32) -> Self {
        let code = include_str!("../contracts/Fibonacci.sol");
        let name = "FibonacciBinet";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: FibonacciBinet::fib3Call::new((U256::from(n),)).abi_encode(),
        }
    }

    pub fn sha1(pre: Vec<u8>) -> Self {
        let code = include_str!("../contracts/SHA1.sol");
        let name = "SHA1";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: SHA1::sha1Call::new((pre,)).abi_encode(),
        }
    }

    pub fn flipper() -> Self {
        let code = include_str!("../contracts/flipper.sol");
        let name = "Flipper";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Flipper::flipCall::new(()).abi_encode(),
        }
    }

    pub fn erc20() -> Self {
        let code = include_str!("../contracts/ERC20.sol");
        let name = "ERC20";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: IERC20::totalSupplyCall::new(()).abi_encode(),
        }
    }

    pub fn block_number() -> Self {
        let code = include_str!("../contracts/Block.sol");
        let name = "Block";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Block::numberCall::new(()).abi_encode(),
        }
    }

    pub fn block_timestamp() -> Self {
        let code = include_str!("../contracts/Block.sol");
        let name = "Block";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Block::timestampCall::new(()).abi_encode(),
        }
    }

    pub fn context_address() -> Self {
        let code = include_str!("../contracts/Context.sol");
        let name = "Context";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Context::address_thisCall::new(()).abi_encode(),
        }
    }

    pub fn context_caller() -> Self {
        let code = include_str!("../contracts/Context.sol");
        let name = "Context";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Context::callerCall::new(()).abi_encode(),
        }
    }

    pub fn division_arithmetics_div(n: U256, d: U256) -> Self {
        let code = include_str!("../contracts/DivisionArithmetics.sol");
        let name = "DivisionArithmetics";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: DivisionArithmetics::divCall::new((n, d)).abi_encode(),
        }
    }

    pub fn division_arithmetics_sdiv(n: I256, d: I256) -> Self {
        let code = include_str!("../contracts/DivisionArithmetics.sol");
        let name = "DivisionArithmetics";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: DivisionArithmetics::sdivCall::new((n, d)).abi_encode(),
        }
    }

    pub fn division_arithmetics_mod(n: U256, d: U256) -> Self {
        let code = include_str!("../contracts/DivisionArithmetics.sol");
        let name = "DivisionArithmetics";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: DivisionArithmetics::modCall::new((n, d)).abi_encode(),
        }
    }

    pub fn division_arithmetics_smod(n: I256, d: I256) -> Self {
        let code = include_str!("../contracts/DivisionArithmetics.sol");
        let name = "DivisionArithmetics";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: DivisionArithmetics::smodCall::new((n, d)).abi_encode(),
        }
    }

    pub fn mstore8(value: U256) -> Self {
        let code = include_str!("../contracts/mStore8.sol");
        let name = "MStore8";

        Self {
            name,
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: MStore8::mStore8Call::new((value,)).abi_encode(),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::U256;
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
            Contract::baseline as fn() -> Contract,
            Contract::flipper as fn() -> Contract,
            (|| Contract::odd_product(0)) as fn() -> Contract,
            (|| Contract::fib_iterative(0)) as fn() -> Contract,
            Contract::erc20 as fn() -> Contract,
            (|| Contract::sha1(Vec::new())) as fn() -> Contract,
            (|| Contract::division_arithmetics_div(U256::ZERO, U256::ZERO)) as fn() -> Contract,
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
