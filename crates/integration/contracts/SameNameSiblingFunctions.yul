object "SameNameSiblingFunctions" {
    code {
        datacopy(0, dataoffset("SameNameSiblingFunctions_deployed"), datasize("SameNameSiblingFunctions_deployed"))
        return(0, datasize("SameNameSiblingFunctions_deployed"))
    }
    object "SameNameSiblingFunctions_deployed" {
        code {
            {
                function f(n) -> r { r := 1 if n { r := add(f(sub(n, 1)), 1) } }
                mstore(0, f(calldataload(0)))
            }
            {
                function f(n) -> r { r := 1 if n { r := mul(f(sub(n, 1)), 2) } }
                mstore(32, f(calldataload(0)))
            }
            return(0, 64)
        }
    }
}
