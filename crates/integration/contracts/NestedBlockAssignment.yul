object "NestedBlockAssignment" {
    code {
        datacopy(0, dataoffset("NestedBlockAssignment_deployed"), datasize("NestedBlockAssignment_deployed"))
        return(0, datasize("NestedBlockAssignment_deployed"))
    }
    object "NestedBlockAssignment_deployed" {
        code {
            let value := 0
            { value := 1 }
            mstore(0, value)
            return(0, 32)
        }
    }
}
