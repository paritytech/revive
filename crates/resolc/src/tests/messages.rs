//! The Solidity compiler unit tests for messages.

use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::{ResolcWarning, SolcStandardJsonOutput};

use crate::tests::{build_solidity, build_solidity_with_options, sources};

pub const SEND_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SendExample {
    address payable public recipient;

    constructor(address payable _recipient) {
        recipient = _recipient;
    }

    function forwardEther() external payable {
        bool success = recipient.send(msg.value);
        require(success, "Failed to send Ether");
    }
}"#;

fn contains_warning(build: SolcStandardJsonOutput, warning: ResolcWarning) -> bool {
    build
        .errors
        .iter()
        .any(|error| error.severity == "warning" && error.message.contains(warning.as_message()))
}

#[test]
fn send() {
    let build = build_solidity(sources(&[("test.sol", SEND_TEST_SOURCE)])).unwrap();
    assert!(contains_warning(build, ResolcWarning::SendAndTransfer));
}

#[test]
fn send_suppressed() {
    let build = build_solidity_with_options(
        sources(&[("test.sol", SEND_TEST_SOURCE)]),
        Default::default(),
        Default::default(),
        OptimizerSettings::cycles(),
        true,
        vec![ResolcWarning::SendAndTransfer],
    )
    .unwrap();
    assert!(!contains_warning(build, ResolcWarning::SendAndTransfer));
}

pub const TRANSFER_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TransferExample {
    address payable public recipient;

    constructor(address payable _recipient) {
        recipient = _recipient;
    }

    function forwardEther() external payable {
        recipient.transfer(msg.value);
    }
}"#;

#[test]
fn transfer() {
    let build = build_solidity(sources(&[("test.sol", TRANSFER_TEST_SOURCE)])).unwrap();
    assert!(contains_warning(build, ResolcWarning::SendAndTransfer));
}

#[test]
fn transfer_suppressed() {
    let build = build_solidity_with_options(
        sources(&[("test.sol", TRANSFER_TEST_SOURCE)]),
        Default::default(),
        Default::default(),
        OptimizerSettings::cycles(),
        true,
        vec![ResolcWarning::SendAndTransfer],
    )
    .unwrap();
    assert!(!contains_warning(build, ResolcWarning::SendAndTransfer))
}

pub const TX_ORIGIN_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TxOriginExample {
    function isOriginSender() public view returns (bool) {
        return tx.origin == msg.sender;
    }
}"#;

#[test]
fn tx_origin() {
    let build = build_solidity(sources(&[("test.sol", TX_ORIGIN_TEST_SOURCE)])).unwrap();
    assert!(contains_warning(build, ResolcWarning::TxOrigin));
}

#[test]
fn tx_origin_suppressed() {
    let build = build_solidity_with_options(
        sources(&[("test.sol", TX_ORIGIN_TEST_SOURCE)]),
        Default::default(),
        Default::default(),
        OptimizerSettings::cycles(),
        true,
        vec![ResolcWarning::TxOrigin],
    )
    .unwrap();
    assert!(!contains_warning(build, ResolcWarning::TxOrigin))
}
