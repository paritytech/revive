// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

contract ForConditionDeduplicatedCall {
    fallback() external {
        assembly {
            function isBelowEleven(value) -> result {
                value := add(mul(value, 2), 1)
                value := add(mul(value, 2), 1)
                result := lt(value, 11)
            }
            function isBelowThirteen(value) -> result {
                value := add(mul(value, 2), 1)
                value := add(mul(value, 2), 1)
                result := lt(value, 13)
            }
            for {} isBelowEleven(calldataload(0)) {} {
                mstore(0, isBelowThirteen(calldataload(0)))
                break
            }
            return(0, 32)
        }
    }
}
