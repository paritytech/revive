object "ImmutableNonUtf8Name" {
    code {
        datacopy(0, dataoffset("ImmutableNonUtf8Name_deployed"), datasize("ImmutableNonUtf8Name_deployed"))
        setimmutable(0, "\xff", 7)
        setimmutable(0, "a", 9)
        return(0, datasize("ImmutableNonUtf8Name_deployed"))
    }
    object "ImmutableNonUtf8Name_deployed" {
        code {
            mstore(0, loadimmutable("a"))
            return(0, 32)
        }
    }
}
