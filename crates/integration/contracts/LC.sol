// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract LC {
    function f(uint256 c, uint256 w) internal pure returns (uint256) {
        if (c != 0) return w;
        return 5;
    }
    function g(uint256 c, uint256 w) internal pure returns (bool y) {
        y = f(c, w) < 300;
        if (y) return y;
    }
    fallback(bytes calldata d) external returns (bytes memory) {
        (uint256 w, uint256 c1, uint256 c2) = abi.decode(d, (uint256, uint256, uint256));
        uint256 z = f(c2, w);
        return abi.encode(g(c1, w), g(c2, w), z);
    }
}
