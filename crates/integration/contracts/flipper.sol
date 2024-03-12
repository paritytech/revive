contract Flipper {
    bool coin;

    function flip() public payable {
        coin = !coin;
    }
}
