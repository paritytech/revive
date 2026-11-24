object "ImmutableEscapedName" {
    code {
        datacopy(0, dataoffset("ImmutableEscapedName_deployed"), datasize("ImmutableEscapedName_deployed"))
        setimmutable(0, "\x61", 7)
        return(0, datasize("ImmutableEscapedName_deployed"))
    }
    object "ImmutableEscapedName_deployed" {
        code {
            mstore(0, loadimmutable(hex"61"))
            return(0, 32)
        }
    }
}
