library L { function f(uint256 x) external pure returns (uint256) { return x * 3; } }
contract K { function a() external pure returns (address) { return address(L); } }
