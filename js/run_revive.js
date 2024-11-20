import { createRequire } from 'module';
const require = createRequire(import.meta.url);
import solc from 'solc';

// Import the Emscripten module
import Module from './resolc.js';

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
  const m = await Module();
  m.solc = solc;

  // Set input data for stdin
  m.setStdinData(JSON.stringify(compilerStandardJsonInput));

  var stdoutString = "";
  m.setStdoutCallback(function(char) {
      if (char.charCodeAt(0) === '\n') {
        console.log("new line")
        exit
      }
      stdoutString += char;
  });

  var stderrString = "";
  m.setStderrCallback(function(char) {
    stderrString += char;
  });

  // Compile the Solidity source code
  let x = m.callMain(['--standard-json']);
  console.log(stdoutString)
  console.error(stderrString)
}

runCompiler().catch(err => {
  console.error('Error:', err);
});
