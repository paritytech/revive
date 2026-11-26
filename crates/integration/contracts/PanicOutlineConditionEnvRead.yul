object "PanicOutlineConditionEnvRead" {
    code {
        datacopy(0, dataoffset("PanicOutlineConditionEnvRead_deployed"), datasize("PanicOutlineConditionEnvRead_deployed"))
        return(0, datasize("PanicOutlineConditionEnvRead_deployed"))
    }
    object "PanicOutlineConditionEnvRead_deployed" {
        code {
            function check() -> r {
                mstore(0, 0x4e487b7100000000000000000000000000000000000000000000000000000000)
                let value := callvalue()
                mstore(4, 0x11)
                revert(0, 0x24)
            }
            for { } iszero(check()) { } { sstore(0, callvalue()) }
        }
    }
}
