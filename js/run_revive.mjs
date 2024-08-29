import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';
import vm from 'vm';
import { createRequire } from 'module';
const require = createRequire(import.meta.url);

// Import the Emscripten module
import ModuleFactory from './resolc.mjs';

async function initializeSolc() {
    // Load soljson.js
    const soljsonPath = path.join('./', 'soljson.js');
    const soljsonCode = readFileSync(soljsonPath, 'utf8');

    // Create a new VM context and run soljson.js in it
    const soljsonContext = { Module: {} };
    vm.createContext(soljsonContext); // Create a new context
    vm.runInContext(soljsonCode, soljsonContext); // Execute soljson.js in the new context

    // Return the initialized soljson module
    return soljsonContext.Module;
}

async function runCompiler() {
    const soljson = await initializeSolc();
    const Module = await ModuleFactory();

    // Expose soljson in the Module context
    Module.soljson = soljson;

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
