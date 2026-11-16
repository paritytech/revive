// SPDX-License-Identifier: MIT

pragma solidity >=0.8.0;

contract ForConditionFreePointerWriter {
    fallback() external {
        assembly {
            function setFreePointer() -> keepLooping {
                mstore(0x40, 0xa0)
            }
            function setInLoopCondition() {
                for {} setFreePointer() {} {}
            }
            setInLoopCondition()
            mstore(0, mload(0x40))
            return(0, 32)
        }
    }
}
