// solc 0.8.37 --ir-optimized --optimize src/tests/data/solidity/factory_dependency_nested_object.sol
/// @use-src 0:"src/tests/data/solidity/factory_dependency_nested_object.sol"
object "P_19" {
    code {
        {
            /// @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }"
            let _1 := memoryguard(0x80)
            mstore(64, _1)
            if callvalue() { revert(0, 0) }
            let _2 := datasize("P_19_deployed")
            codecopy(_1, dataoffset("P_19_deployed"), _2)
            return(_1, _2)
        }
    }
    /// @use-src 0:"src/tests/data/solidity/factory_dependency_nested_object.sol"
    object "P_19_deployed" {
        code {
            {
                /// @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }"
                let _1 := memoryguard(0x80)
                mstore(64, _1)
                if iszero(lt(calldatasize(), 4))
                {
                    if eq(0x2f81b0a0, shr(224, calldataload(0)))
                    {
                        if callvalue() { revert(0, 0) }
                        if slt(add(calldatasize(), not(3)), 0) { revert(0, 0) }
                        /// @src 0:108:115  "new K()"
                        let _2 := datasize("K_4")
                        let _3 := add(_1, _2)
                        if or(gt(_3, 0xffffffffffffffff), lt(_3, _1))
                        {
                            /// @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }"
                            mstore(0, shl(224, 0x4e487b71))
                            mstore(4, 0x41)
                            revert(0, 0x24)
                        }
                        /// @src 0:108:115  "new K()"
                        datacopy(_1, dataoffset("K_4"), _2)
                        let expr_address := create(/** @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }" */ 0, /** @src 0:108:115  "new K()" */ _1, sub(_3, _1))
                        if iszero(expr_address)
                        {
                            /// @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }"
                            let pos := mload(64)
                            returndatacopy(pos, 0, returndatasize())
                            revert(pos, returndatasize())
                        }
                        let memPos := mload(64)
                        mstore(memPos, and(/** @src 0:100:116  "address(new K())" */ expr_address, /** @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }" */ sub(shl(160, /** @src 0:108:115  "new K()" */ 1), 1)))
                        /// @src 0:37:121  "contract P { function mk() external returns (address) { return address(new K()); } }"
                        return(memPos, 32)
                    }
                }
                revert(0, 0)
            }
        }
        /// @use-src 0:"src/tests/data/solidity/factory_dependency_nested_object.sol"
        object "K_4" {
            code {
                {
                    /// @src 0:0:36  "contract K { uint256 public v = 7; }"
                    let _1 := memoryguard(0x80)
                    mstore(64, _1)
                    if callvalue() { revert(0, 0) }
                    sstore(/** @src 0:32:33  "7" */ 0x00, 0x07)
                    /// @src 0:0:36  "contract K { uint256 public v = 7; }"
                    let _2 := datasize("K_4_deployed")
                    codecopy(_1, dataoffset("K_4_deployed"), _2)
                    return(_1, _2)
                }
            }
            /// @use-src 0:"src/tests/data/solidity/factory_dependency_nested_object.sol"
            object "K_4_deployed" {
                code {
                    {
                        /// @src 0:0:36  "contract K { uint256 public v = 7; }"
                        let _1 := memoryguard(0x80)
                        mstore(64, _1)
                        if iszero(lt(calldatasize(), 4))
                        {
                            if eq(0x7c2efcba, shr(224, calldataload(0)))
                            {
                                if callvalue() { revert(0, 0) }
                                if slt(add(calldatasize(), not(3)), 0) { revert(0, 0) }
                                mstore(_1, sload(0))
                                return(_1, 32)
                            }
                        }
                        revert(0, 0)
                    }
                }
                data ".metadata" hex"a264697066735822122033635a5e157e493f8335a094bffd8f41ae34e8025d755b3a8ae9dc36603f510664736f6c63430008250033"
            }
        }
        data ".metadata" hex"a264697066735822122098ff7d7667f1889a4f7fed09b13869dbb95240199d269a8f3c82ff8e4c50169264736f6c63430008250033"
    }
}
