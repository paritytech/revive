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
                // Invalid: hex literal with odd number of nibbles.
                let x := hex"abc"
                mstore(0, x)
                return(0, 32)
            }
        }
    }
}