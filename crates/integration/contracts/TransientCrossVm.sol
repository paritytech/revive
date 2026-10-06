// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// Transient state variables shared through delegatecall: an EVM-bytecode proxy and a resolc
// implementation (or the other way round) on the same pallet-revive chain.
contract TransientCrossVm {
    uint256 transient lock;   // transient slot 0
    uint256 transient amount; // transient slot 1

    function enter(address impl, uint256 v) external returns (uint256, uint256) {
        lock = 1;
        amount = v;
        (bool ok, bytes memory r) = impl.delegatecall(abi.encodeCall(this.peek, ()));
        require(ok);
        return abi.decode(r, (uint256, uint256));
    }

    function peek() external view returns (uint256, uint256) {
        return (lock, amount);
    }
}
