object "Test" {
    code {
        {
            let size := datasize("Test_deployed")
            codecopy(0, dataoffset("Test_deployed"), size)
            return(0, size)
        }
    }
    object "Test_deployed" {
        code {
            {
                switch calldataload(0)
                case 0 {
                    function f() -> ret {
                        ret := 1
                    }
                    mstore(0, f())
                    return(0, 32)
                }
                case 1 {
                    function f() -> ret {
                        ret := 2
                    }
                    mstore(0, f())
                    return(0, 32)
                }
            }
        }
    }
}
