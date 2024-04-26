use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall};

#[derive(Clone)]
pub struct Contract {
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

impl Contract {
    pub fn baseline() -> Self {
        let code = include_str!("../contracts/Baseline.sol");
        let name = "Baseline";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Baseline::baselineCall::new(()).abi_encode(),
        }
    }

    pub fn odd_product(n: i32) -> Self {
        let code = include_str!("../contracts/Computation.sol");
        let name = "Computation";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Computation::odd_productCall::new((n,)).abi_encode(),
        }
    }

    pub fn triangle_number(n: i64) -> Self {
        let code = include_str!("../contracts/Computation.sol");
        let name = "Computation";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Computation::triangle_numberCall::new((n,)).abi_encode(),
        }
    }

    pub fn fib_recursive(n: u32) -> Self {
        let code = include_str!("../contracts/Fibonacci.sol");
        let name = "FibonacciRecursive";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: FibonacciRecursive::fib3Call::new((U256::from(n),)).abi_encode(),
        }
    }

    pub fn fib_iterative(n: u32) -> Self {
        let code = include_str!("../contracts/Fibonacci.sol");
        let name = "FibonacciIterative";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: FibonacciIterative::fib3Call::new((U256::from(n),)).abi_encode(),
        }
    }

    pub fn fib_binet(n: u32) -> Self {
        let code = include_str!("../contracts/Fibonacci.sol");
        let name = "FibonacciBinet";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: FibonacciBinet::fib3Call::new((U256::from(n),)).abi_encode(),
        }
    }

    pub fn sha1(pre: Vec<u8>) -> Self {
        let code = include_str!("../contracts/SHA1.sol");
        let name = "SHA1";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: SHA1::sha1Call::new((pre,)).abi_encode(),
        }
    }

    pub fn flipper() -> Self {
        let code = include_str!("../contracts/flipper.sol");
        let name = "Flipper";

        Self {
            evm_runtime: crate::compile_evm_bin_runtime(name, code),
            pvm_runtime: crate::compile_blob(name, code),
            calldata: Flipper::flipCall::new(()).abi_encode(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{de::Deserialize, Serialize};
    use std::{collections::HashMap, fs::File};

    use super::Contract;

    #[test]
    fn codesize() {
        let path = "codesize.json";

        let existing = File::open(path)
            .map(|file| {
                HashMap::<String, usize>::deserialize(&mut serde_json::Deserializer::from_reader(
                    file,
                ))
                .expect("should be able to deserialze codesize data")
            })
            .ok();

        let sizes = HashMap::from([
            ("Baseline", Contract::baseline().pvm_runtime.len()),
            ("Flipper", Contract::flipper().pvm_runtime.len()),
            ("Computation", Contract::odd_product(0).pvm_runtime.len()),
            ("Fibonacci", Contract::fib_iterative(0).pvm_runtime.len()),
        ]);

        for (name, bytes) in sizes.iter() {
            let change = existing
                .as_ref()
                .and_then(|map| map.get(*name))
                .map(|old| {
                    let new = *bytes as f32;
                    let old = *old as f32;
                    let p = (new - old) / new * 100.0;
                    format!("({p}% change from {old} bytes)")
                })
                .unwrap_or_default();

            println!("{name}: {bytes} bytes {change}");
        }

        sizes
            .serialize(&mut serde_json::Serializer::pretty(
                File::create(path).unwrap(),
            ))
            .unwrap_or_else(|err| panic!("can not write codesize data to '{}': {}", path, err));
    }
}
