object "UnicodeSurrogateEscape" {
    code {
        datacopy(0, dataoffset("UnicodeSurrogateEscape_deployed"), datasize("UnicodeSurrogateEscape_deployed"))
        return(0, datasize("UnicodeSurrogateEscape_deployed"))
    }
    object "UnicodeSurrogateEscape_deployed" {
        code {
            mstore(0, "\ud800")
            return(0, 32)
        }
    }
}
