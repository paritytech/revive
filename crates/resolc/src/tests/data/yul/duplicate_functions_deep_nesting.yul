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

                    switch calldataload(32)
                    case 0 {
                        function g() -> ret {
                            ret := 10
                        }
                        mstore(0, add(f(), g()))
                        return(0, 32)
                    }
                    case 1 {
                        function g() -> ret {
                            ret := 20
                        }
                        mstore(0, add(f(), g()))
                        return(0, 32)
                    }
                }
                case 1 {
                    function f() -> ret {
                        ret := 2
                    }

                    switch calldataload(32)
                    case 0 {
                        function g() -> ret {
                            ret := 30
                        }
                        mstore(0, add(f(), g()))
                        return(0, 32)
                    }
                    case 1 {
                        function g() -> ret {
                            ret := 40
                        }
                        mstore(0, add(f(), g()))
                        return(0, 32)
                    }
                }
            }
        }
    }
}
