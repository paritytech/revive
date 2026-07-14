pragma solidity ^0.8.0;
contract C {
  function h0(uint256 a, uint256 b) internal returns (uint256 r) {
    if (true) { assembly { r := gt(r, b) } return r; }
  }
  function h1(uint256 a, uint256 b) internal returns (uint256 r) {
    for (uint256 i = 0; i < 1; i++) {}
    if (a >= b) { return a; }
  }
  function run(uint256 s) external returns (uint256) {
    uint256 acc = s;
    acc = h0(acc, acc);
    acc = h1(acc, acc);
  }
}
