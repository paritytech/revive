object "Test" {
    code {
        {
            return(0, 0)
        }
    }
    object "Test_deployed" {
        code {
            {
                mstore(1, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)
                return(1, 32)
            }
        }
    }
}
