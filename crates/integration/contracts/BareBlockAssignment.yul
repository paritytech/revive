object "BareBlockAssignment" {
    code {
        datacopy(0, dataoffset("BareBlockAssignment_deployed"), datasize("BareBlockAssignment_deployed"))
        return(0, datasize("BareBlockAssignment_deployed"))
    }
    object "BareBlockAssignment_deployed" {
        code {
            let value := 0
            { value := 1 }
            mstore(0, value)
            return(0, 32)
        }
    }
}
