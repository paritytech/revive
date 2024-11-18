import { createRequire } from 'module';
const require = createRequire(import.meta.url);
import solc from 'solc';

// Import the Emscripten module
import ModuleFactory from './resolc.js';

async function runCompiler() {
    const Module = await ModuleFactory();
    Module.solc = solc;

    // Create input Solidity source code
    const input = `
// SPDX-License-Identifier: MIT
pragma solidity ^0.8;
contract Baseline {
    function baseline() public payable {}
}`;

    // Write the input Solidity code to the Emscripten file system
    Module.FS.writeFile('./input.sol', input);

    // Compile the Solidity source code
    Module.callMain(['./input.sol', '-O3','--bin']);
}

runCompiler().catch(err => {
    console.error('Error:', err);
});
