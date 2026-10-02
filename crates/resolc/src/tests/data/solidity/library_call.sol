library L { function f(uint256 x) external pure returns (uint256) { return x + 1; } }
contract U { function g(uint256 x) external pure returns (uint256) { return L.f(x); } }
