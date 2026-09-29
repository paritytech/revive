// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// SHA-256 whose round loop carries eight masked uint32 working variables.
// This exercises the newyork loop-carried width join (narrow loop phis, rotates on narrow values) end to end.

/* runner.json
{
    "differential": true,
    "actions": [
        {
            "Instantiate": {
                "code": {
                    "Solidity": {
                        "contract": "Sha256"
                    }
                }
            }
        },
        {
            "Call": {
                "dest": {
                    "Instantiated": 0
                },
                "data": "b189fd4c0000000000000000000000000000000000000000000000000000000000000003"
            }
        }
    ]
}
*/

contract Sha256 {
    bytes constant K =
        hex"428a2f9871374491b5c0fbcfe9b5dba53956c25b59f111f1923f82a4ab1c5ed5"
        hex"d807aa9812835b01243185be550c7dc372be5d7480deb1fe9bdc06a7c19bf174"
        hex"e49b69c1efbe47860fc19dc6240ca1cc2de92c6f4a7484aa5cb0a9dc76f988da"
        hex"983e5152a831c66db00327c8bf597fc7c6e00bf3d5a7914706ca635114292967"
        hex"27b70a852e1b21384d2c6dfc53380d13650a7354766a0abb81c2c92e92722c85"
        hex"a2bfe8a1a81a664bc24b8b70c76c51a3d192e819d6990624f40e3585106aa070"
        hex"19a4c1161e376c082748774c34b0bcb5391c0cb34ed8aa4a5b9cca4f682e6ff3"
        hex"748f82ee78a5636f84c878148cc7020890befffaa4506cebbef9a3f7c67178f2";

    function hash(uint256 index) external pure returns (bytes32) {
        uint32[64] memory w;
        uint32[64] memory kc = roundConstants();
        bytes32 acc = bytes32(0);
        for (uint256 i = 0; i < index; ++i) {
            acc = acc | digestWord(bytes32(i), w, kc);
        }
        return acc;
    }

    function digestWord(
        bytes32 input,
        uint32[64] memory w,
        uint32[64] memory kc
    ) internal pure returns (bytes32) {
        unchecked {
            uint256 value = uint256(input);
            for (uint256 t = 0; t < 8; ++t) {
                w[t] = uint32(value >> (224 - t * 32));
            }
            w[8] = 0x80000000;
            for (uint256 t = 9; t < 15; ++t) {
                w[t] = 0;
            }
            w[15] = 256;

            for (uint256 t = 16; t < 64; ++t) {
                uint32 s0 = rotr(w[t - 15], 7) ^
                    rotr(w[t - 15], 18) ^
                    (w[t - 15] >> 3);
                uint32 s1 = rotr(w[t - 2], 17) ^
                    rotr(w[t - 2], 19) ^
                    (w[t - 2] >> 10);
                w[t] = w[t - 16] + s0 + w[t - 7] + s1;
            }

            uint32 a = 0x6a09e667;
            uint32 b = 0xbb67ae85;
            uint32 c = 0x3c6ef372;
            uint32 d = 0xa54ff53a;
            uint32 e = 0x510e527f;
            uint32 f = 0x9b05688c;
            uint32 g = 0x1f83d9ab;
            uint32 hh = 0x5be0cd19;

            for (uint256 t = 0; t < 64; ++t) {
                uint32 bigS1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
                uint32 ch = (e & f) ^ (~e & g);
                uint32 t1 = hh + bigS1 + ch + kc[t] + w[t];
                uint32 bigS0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
                uint32 maj = (a & b) ^ (a & c) ^ (b & c);
                uint32 t2 = bigS0 + maj;

                hh = g;
                g = f;
                f = e;
                e = d + t1;
                d = c;
                c = b;
                b = a;
                a = t1 + t2;
            }

            return
                bytes32(
                    (uint256(uint32(0x6a09e667) + a) << 224) |
                        (uint256(uint32(0xbb67ae85) + b) << 192) |
                        (uint256(uint32(0x3c6ef372) + c) << 160) |
                        (uint256(uint32(0xa54ff53a) + d) << 128) |
                        (uint256(uint32(0x510e527f) + e) << 96) |
                        (uint256(uint32(0x9b05688c) + f) << 64) |
                        (uint256(uint32(0x1f83d9ab) + g) << 32) |
                        uint256(uint32(0x5be0cd19) + hh)
                );
        }
    }

    function roundConstants() internal pure returns (uint32[64] memory kc) {
        bytes memory k = K;
        unchecked {
            for (uint256 t = 0; t < 64; ++t) {
                uint256 o = t * 4;
                kc[t] =
                    (uint32(uint8(k[o])) << 24) |
                    (uint32(uint8(k[o + 1])) << 16) |
                    (uint32(uint8(k[o + 2])) << 8) |
                    uint32(uint8(k[o + 3]));
            }
        }
    }

    function rotr(uint32 x, uint32 n) internal pure returns (uint32) {
        return (x >> n) | (x << (32 - n));
    }
}
