//!
//! The Solidity compiler unit tests for remappings.
//!

#![cfg(test)]

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::solc::pipeline::Pipeline as SolcPipeline;

pub const CALLEE_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

contract Callable {
    function f(uint a) public pure returns(uint) {
        return a * 2;
    }
}
"#;

pub const CALLER_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT

pragma solidity >=0.4.16;

import "libraries/default/callable.sol";

contract Main {
    function main(Callable callable) public returns(uint) {
        return callable.f(5);
    }
}
"#;

#[test]
fn default() {
    let mut sources = BTreeMap::new();
    sources.insert("./test.sol".to_owned(), CALLER_TEST_SOURCE.to_owned());
    sources.insert("./callable.sol".to_owned(), CALLEE_TEST_SOURCE.to_owned());

    let mut remappings = BTreeSet::new();
    remappings.insert("libraries/default/=./".to_owned());

    super::build_solidity(
        sources,
        BTreeMap::new(),
        Some(remappings),
        SolcPipeline::Yul,
        era_compiler_llvm_context::OptimizerSettings::cycles(),
    )
    .expect("Test failure");
}
