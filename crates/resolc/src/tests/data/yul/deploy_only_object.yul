object "DeployOnly" {
    code {
        {
            mstore(0, 42)
            return(0, 32)
        }
    }
}
