contract DivisionArithmetics {
    function div(uint n, uint d) public pure returns (uint q) {
        assembly {
            q := div(n, d)
        }
    }

    function sdiv(int n, int d) public pure returns (int q) {
        assembly {
            q := sdiv(n, d)
        }
    }

    function mod(uint n, uint d) public pure returns (uint r) {
        assembly {
            r := mod(n, d)
        }
    }

    function smod(int n, int d) public pure returns (int r) {
        assembly {
            r := smod(n, d)
        }
    }
}
