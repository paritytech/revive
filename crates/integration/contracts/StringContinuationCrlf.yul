object "StringContinuationCrlf" {
    code {
        datacopy(0, dataoffset("StringContinuationCrlf_deployed"), datasize("StringContinuationCrlf_deployed"))
        return(0, datasize("StringContinuationCrlf_deployed"))
    }
    object "StringContinuationCrlf_deployed" {
        code {
            mstore(0, "ab\
cd")
            return(0, 32)
        }
    }
}
