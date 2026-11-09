object "UnicodeEscape" {
    code {
        datacopy(0, dataoffset("UnicodeEscape_deployed"), datasize("UnicodeEscape_deployed"))
        return(0, datasize("UnicodeEscape_deployed"))
    }
    object "UnicodeEscape_deployed" {
        code {
            mstore(0, "\u0041B")
            return(0, 32)
        }
    }
}
