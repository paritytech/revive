//! The Solidity compiler unit tests for messages.

use revive_llvm_context::OptimizerSettings;
use revive_solc_json_interface::{ResolcWarning, SolcStandardJsonOutput};

use crate::test_utils::{build_solidity, build_solidity_with_options, sources};

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

pub const TX_ORIGIN_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TxOriginExample {
    function isOriginSender() public view returns (bool) {
        return tx.origin == msg.sender;
    }
}"#;

fn contains_warning(build: SolcStandardJsonOutput, warning: ResolcWarning) -> bool {
    build
        .errors
        .iter()
        .any(|error| error.is_warning() && error.message.contains(warning.as_message()))
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

pub const TX_ORIGIN_MID_LINE_TEST_SOURCE: &str = r#"contract C {
    function f() external view returns (address) {
        return tx.origin;
    }
}
"#;

pub const TX_ORIGIN_LINE_START_TEST_SOURCE: &str = r#"contract C {
    function f() external view returns (address a) {
        a =
tx.origin;
    }
}
"#;

fn warning_message(path: &str, source: &str, warning: ResolcWarning) -> String {
    build_solidity(sources(&[(path, source)]))
        .unwrap()
        .errors
        .into_iter()
        .find(|error| error.is_warning() && error.message.contains(warning.as_message()))
        .unwrap()
        .formatted_message
}

/// The warning location points at `tx.origin` in the middle of a line.
#[test]
fn tx_origin_location_mid_line() {
    let message = warning_message(
        "mid.sol",
        TX_ORIGIN_MID_LINE_TEST_SOURCE,
        ResolcWarning::TxOrigin,
    );
    assert!(message.contains("mid.sol:3:16\n"), "{message}");
    assert!(
        message.contains("\n 3 |         return tx.origin;\n   |                ^^^^^^^^^\n"),
        "{message}"
    );
}

/// The warning location points at `tx.origin` at the start of a line.
#[test]
fn tx_origin_location_line_start() {
    let message = warning_message(
        "bol.sol",
        TX_ORIGIN_LINE_START_TEST_SOURCE,
        ResolcWarning::TxOrigin,
    );
    assert!(message.contains("bol.sol:4:1\n"), "{message}");
    assert!(
        message.contains("\n 4 | tx.origin;\n   | ^^^^^^^^^\n"),
        "{message}"
    );
}

/// The warning location points at `tx.origin` at the start of a line with CRLF line endings.
#[test]
fn tx_origin_location_line_start_crlf() {
    let source = TX_ORIGIN_LINE_START_TEST_SOURCE.replace('\n', "\r\n");
    let message = warning_message("crlf.sol", &source, ResolcWarning::TxOrigin);
    assert!(message.contains("crlf.sol:4:1\n"), "{message}");
    assert!(
        message.contains("\n 4 | tx.origin;\n   | ^^^^^^^^^\n"),
        "{message}"
    );
}
