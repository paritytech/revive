object "BareBlockLeave" {
    code {
        datacopy(0, dataoffset("BareBlockLeave_deployed"), datasize("BareBlockLeave_deployed"))
        return(0, datasize("BareBlockLeave_deployed"))
    }
    object "BareBlockLeave_deployed" {
        code {
            function store() {
                for { } 1 { } {
                    mstore(0, 7)
                    { leave }
                    mstore(0, 9)
                }
            }
            store()
            return(0, 32)
        }
    }
}
