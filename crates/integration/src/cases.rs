use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall};

#[derive(Clone)]
pub struct Contract {
    pub evm_runtime: Vec<u8>,
    pub pvm_runtime: Vec<u8>,
    pub calldata: Vec<u8>,
}

sol!(contract Baseline { function baseline() public payable; });

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
}
