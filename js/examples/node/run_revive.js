const soljson = require('solc/soljson');
const createRevive = require('./resolc.js');

const compilerStandardJsonInput = {
    language: 'Solidity',
    sources: {
      'MyContract.sol': {
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
        '*': {
          '*': ['abi'],
        },
      },
    },
  };

async function runCompiler() {
  const m = createRevive();
  m.soljson = soljson;

  // Set input data for stdin
  m.writeToStdin(JSON.stringify(compilerStandardJsonInput));

  // Compile the Solidity source code
  let x = m.callMain(['--standard-json']);
  console.log("Stdout: " + m.readFromStdout());
  console.error("Stderr: " + m.readFromStderr());
}

runCompiler().catch(err => {
  console.error('Error:', err);
});
