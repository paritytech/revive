contract MSize {
    uint[] public data;

    function mSize() public pure returns (uint size) {
        assembly {
            size := msize()
        }
    }
}
