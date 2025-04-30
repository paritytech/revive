const { compile } = require("./revive.js");

const compilerStandardJsonInput = {
  language: "Solidity",
  sources: {
    "MyContract.sol": {
      content: `
          // SPDX-License-Identifier: UNLICENSED
          pragma solidity ^0.8.0;
          contract MyContract {
            function greet() public pure returns (string memory) {
              return "Hello";
            }
          }
        `,
    },
  },
  settings: {
    optimizer: {
      enabled: true,
      runs: 200,
    },
    outputSelection: {
      "*": {
        "*": ["abi"],
      },
    },
  },
};

async function runCompiler() {
  let output = await compile(compilerStandardJsonInput);
  console.log("Output: " + output);
}

runCompiler().catch((err) => {
  console.error("Error:", err);
});
