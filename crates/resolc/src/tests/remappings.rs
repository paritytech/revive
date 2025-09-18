//! The Solidity compiler unit tests for remappings.

#[test]
fn default() {
    let callee_code = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

contract Callable {
    function f(uint a) public pure returns(uint) {
        return a * 2;
    }
}
"#;

    let caller_code = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

import "libraries/default/callable.sol";

contract Main {
    function main(Callable callable) public returns(uint) {
        return callable.f(5);
    }
}
"#;

    super::build_solidity(
        super::sources(&[("./test.sol", caller_code), ("./callable.sol", callee_code)]),
        Default::default(),
        ["libraries/default/=./".to_owned()].into(),
        revive_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Test failure");
}
