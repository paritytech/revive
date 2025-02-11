//! The Solidity compiler unit tests for messages.

#![cfg(test)]

use std::collections::BTreeMap;

use crate::warning::Warning;

pub const ECRECOVER_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ECRecoverExample {
    function recoverAddress(
        bytes32 messageHash,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) public pure returns (address) {
        return ecrecover(messageHash, v, r, s);
    }
}
    "#;

#[test]
fn ecrecover() {
    assert!(
        super::check_solidity_warning(
            ECRECOVER_TEST_SOURCE,
            "Warning: It looks like you are using 'ecrecover' to validate a signature of a user account.",
            BTreeMap::new(),
            false,
            None,
        ).expect("Test failure")
    );
}

#[test]
fn ecrecover_suppressed() {
    assert!(
        !super::check_solidity_warning(
            ECRECOVER_TEST_SOURCE,
            "Warning: It looks like you are using 'ecrecover' to validate a signature of a user account.",
            BTreeMap::new(),
            false,
            Some(vec![Warning::EcRecover]),
        ).expect("Test failure")
    );
}

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
}
"#;

pub const BALANCE_CALLS_MESSAGE: &str =
    "Warning: It looks like you are using '<address payable>.send/transfer(<X>)'";

#[test]
fn send() {
    assert!(super::check_solidity_warning(
        SEND_TEST_SOURCE,
        BALANCE_CALLS_MESSAGE,
        BTreeMap::new(),
        false,
        None,
    )
    .expect("Test failure"));
}

#[test]
fn send_suppressed() {
    assert!(!super::check_solidity_warning(
        SEND_TEST_SOURCE,
        BALANCE_CALLS_MESSAGE,
        BTreeMap::new(),
        false,
        Some(vec![Warning::SendTransfer]),
    )
    .expect("Test failure"));
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
}
    "#;

#[test]
fn transfer() {
    assert!(super::check_solidity_warning(
        TRANSFER_TEST_SOURCE,
        BALANCE_CALLS_MESSAGE,
        BTreeMap::new(),
        false,
        None,
    )
    .expect("Test failure"));
}

#[test]
fn transfer_suppressed() {
    assert!(!super::check_solidity_warning(
        TRANSFER_TEST_SOURCE,
        BALANCE_CALLS_MESSAGE,
        BTreeMap::new(),
        false,
        Some(vec![Warning::SendTransfer]),
    )
    .expect("Test failure"));
}

pub const EXTCODESIZE_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ExternalCodeSize {
    function getExternalCodeSize(address target) public view returns (uint256) {
        uint256 codeSize;
        assembly {
            codeSize := extcodesize(target)
        }
        return codeSize;
    }
}
    "#;

#[test]
fn extcodesize() {
    assert!(super::check_solidity_warning(
        EXTCODESIZE_TEST_SOURCE,
        "Warning: Your code or one of its dependencies uses the 'extcodesize' instruction,",
        BTreeMap::new(),
        false,
        None,
    )
    .expect("Test failure"));
}

#[test]
fn extcodesize_suppressed() {
    assert!(!super::check_solidity_warning(
        EXTCODESIZE_TEST_SOURCE,
        "Warning: Your code or one of its dependencies uses the 'extcodesize' instruction,",
        BTreeMap::new(),
        false,
        Some(vec![Warning::ExtCodeSize]),
    )
    .expect("Test failure"));
}

pub const TX_ORIGIN_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TxOriginExample {
    function isOriginSender() public view returns (bool) {
        return tx.origin == msg.sender;
    }
}
    "#;

#[test]
fn tx_origin() {
    assert!(super::check_solidity_warning(
        TX_ORIGIN_TEST_SOURCE,
        "Warning: You are checking for 'tx.origin' in your code, which might lead to",
        BTreeMap::new(),
        false,
        None,
    )
    .expect("Test failure"));
}

#[test]
fn tx_origin_suppressed() {
    assert!(!super::check_solidity_warning(
        TX_ORIGIN_TEST_SOURCE,
        "Warning: You are checking for 'tx.origin' in your code, which might lead to",
        BTreeMap::new(),
        false,
        Some(vec![Warning::TxOrigin]),
    )
    .expect("Test failure"));
}

pub const TX_ORIGIN_ASSEMBLY_TEST_SOURCE: &str = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TxOriginExample {
    function isOriginSender() public view returns (bool) {
        address txOrigin;
        address sender = msg.sender;

        assembly {
            txOrigin := origin() // Get the transaction origin using the 'origin' instruction
        }

        return txOrigin == sender;
    }
}
    "#;

#[test]
fn tx_origin_assembly() {
    assert!(super::check_solidity_warning(
        TX_ORIGIN_ASSEMBLY_TEST_SOURCE,
        "Warning: You are checking for 'tx.origin' in your code, which might lead to",
        BTreeMap::new(),
        false,
        None,
    )
    .expect("Test failure"));
}

#[test]
fn tx_origin_assembly_suppressed() {
    assert!(!super::check_solidity_warning(
        TX_ORIGIN_ASSEMBLY_TEST_SOURCE,
        "Warning: You are checking for 'tx.origin' in your code, which might lead to",
        BTreeMap::new(),
        false,
        Some(vec![Warning::TxOrigin]),
    )
    .expect("Test failure"));
}
