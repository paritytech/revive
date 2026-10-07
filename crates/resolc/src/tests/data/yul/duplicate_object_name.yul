object "A" {
    code {
        datacopy(0, dataoffset("A_deployed"), datasize("A_deployed"))
        return(0, datasize("A_deployed"))
    }
    object "A_deployed" {
        code {
            datacopy(0, dataoffset("K"), datasize("K"))
            let child := create(0, 0, datasize("K"))
            if iszero(call(gas(), child, 0, 0, 0, 0, 32)) { revert(0, 0) }
            return(0, 32)
        }
        object "K" {
            code {
                datacopy(0, dataoffset("K_deployed"), datasize("K_deployed"))
                return(0, datasize("K_deployed"))
            }
            object "K_deployed" { code { mstore(0, 111) return(0, 32) } }
        }
    }
    object "K" {
        code {
            datacopy(0, dataoffset("K_deployed"), datasize("K_deployed"))
            return(0, datasize("K_deployed"))
        }
        object "K_deployed" { code { mstore(0, 222) return(0, 32) } }
    }
}
