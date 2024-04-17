contract Value {
    function value() public payable returns (uint ret) {
        ret = msg.value;
    }
}
