contract K {
    function f(uint256 n) external pure returns (uint256) {
        if (n > 1) return 1;
        return 2;
    }
}

contract P {
    address public k;

    constructor() {
        k = address(new K());
    }

    function mk() external returns (address) {
        return address(new K());
    }
}
