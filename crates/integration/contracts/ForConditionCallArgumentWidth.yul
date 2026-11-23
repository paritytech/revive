object "ForConditionCallArgumentWidth" {
    code {
        datacopy(0, dataoffset("ForConditionCallArgumentWidth_deployed"), datasize("ForConditionCallArgumentWidth_deployed"))
        return(0, datasize("ForConditionCallArgumentWidth_deployed"))
    }
    object "ForConditionCallArgumentWidth_deployed" {
        code {
            mstore(0, is_small(5))
            for {} is_small(calldataload(0)) {} {
                mstore(0, 7)
                break
            }
            return(0, 32)

            function is_small(value) -> result {
                result := lt(value, 10)
                if result { leave }
            }
        }
    }
}
