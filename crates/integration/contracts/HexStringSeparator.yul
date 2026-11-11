object "HexStringSeparator" {
    code {
        datacopy(0, dataoffset("HexStringSeparator_deployed"), datasize("HexStringSeparator_deployed"))
        return(0, datasize("HexStringSeparator_deployed"))
    }
    object "HexStringSeparator_deployed" {
        code {
            mstore(0, hex"00_11")
            return(0, 32)
        }
    }
}
