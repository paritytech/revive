import { createRequire } from 'module';
const require = createRequire(import.meta.url);
import solc from 'solc';

// Import the Emscripten module
import ModuleFactory from './resolc.js';

// Solidity source code
const input = `
  // SPDX-License-Identifier: MIT
  pragma solidity ^0.8;
  contract Baseline {
    function baseline() public payable {}
  }`;

async function runCompiler() {
  const Module = await ModuleFactory();
  Module.solc = solc;

  // Write the input Solidity code to the Emscripten file system
  Module.FS.writeFile('./input.sol', input);

  // Compile the Solidity source code
  Module.callMain(['./input.sol', '-O3','--bin']);
}

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

async function runCompilerWithStandardJson() {
  const Module = await ModuleFactory();
  Module.solc = solc;

  // Write the input Solidity code to the Emscripten file system
  Module.FS.writeFile('/in',  JSON.stringify(compilerStandardJsonInput));

  // Compile the Solidity source code
  Module.callMain(['--standard-json']);
}

runCompiler().catch(err => {
  console.error('Error:', err);
});

runCompilerWithStandardJson().catch(err => {
  console.error('Error:', err);
});
