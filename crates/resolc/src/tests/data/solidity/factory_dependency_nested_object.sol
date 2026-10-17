contract K { uint256 public v = 7; }
contract P { function mk() external returns (address) { return address(new K()); } }
