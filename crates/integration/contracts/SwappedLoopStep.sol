// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract SwappedLoopStep {
    function advanceCounterOnly(uint256 start) internal pure returns (uint256) {
        uint256 counter = start;
        uint256 result = start;
        for (; counter < 10; (counter, result) = (counter + 1, result)) {}
        return result;
    }

    function swapAndIncrement(uint256 start) internal pure returns (uint256) {
        uint256 counter = start;
        uint256 result = start;
        for (; counter < 11; (counter, result) = (result + 1, counter)) {}
        return result;
    }

    fallback(bytes calldata input) external returns (bytes memory) {
        uint256 start = abi.decode(input, (uint256));
        return
            abi.encode(
                advanceCounterOnly(start) + advanceCounterOnly(start),
                swapAndIncrement(start) + swapAndIncrement(start)
            );
    }
}
