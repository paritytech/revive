//! The Solidity compiler unit tests for factory dependencies.

#[test]
fn default() {
    let caller_code = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

import "./callable.sol";

contract Main {
    function main() external returns(uint256) {
        Callable callable = new Callable();

        callable.set(10);
        return callable.get();
    }
}"#;

    let callee_code = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

contract Callable {
    uint256 value;

    function set(uint256 x) external {
        value = x;
    }

    function get() external view returns(uint256) {
        return value;
    }
}"#;

    let output = super::build_solidity(
        super::sources(&[("main.sol", caller_code), ("callable.sol", callee_code)]),
        Default::default(),
        Default::default(),
        revive_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Build failure");

    assert_eq!(
        output
            .contracts
            .get("main.sol")
            .expect("Missing file `main.sol`")
            .get("Main")
            .expect("Missing contract `main.sol:Main`")
            .factory_dependencies
            .len(),
        1,
        "Expected 1 factory dependency in `main.sol:Main`"
    );
    assert_eq!(
        output
            .contracts
            .get("callable.sol")
            .expect("Missing file `callable.sol`")
            .get("Callable")
            .expect("Missing contract `callable.sol:Callable`")
            .factory_dependencies
            .len(),
        0,
        "Expected 0 factory dependencies in `callable.sol:Callable`"
    );
}
