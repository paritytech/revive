object "Test" {
    code {
        let size := datasize("Test_deployed.Test")
        datacopy(0, dataoffset("Test_deployed.Test"), size)
        return(0, size)
    }
    object "Test_deployed" {
        code {
            stop()
        }
        object "Test" {
            code {
                revert(0, 0)
            }
        }
    }
}
