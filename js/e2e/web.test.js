const { test, expect } = require('@playwright/test');

const validCompilerInput = {
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
        '*': ['abi', 'evm.bytecode'],
      },
    },
  },
};

async function runWorker(page, input) {
  return await page.evaluate((input) => {
    return new Promise((resolve, reject) => {
      const worker = new Worker('worker.js'); // Path to your worker.js file
      worker.postMessage(JSON.stringify(input));

      worker.onmessage = (event) => {
        resolve(event.data.output);
        worker.terminate(); // Clean up the worker
      };

      worker.onerror = (error) => {
        reject(error.message || error); // Provide error message for clarity
        worker.terminate(); // Clean up the worker
      };
    });
  }, input); // Pass the input as an argument to the function
}

test('Test  browser', async ({ page }) => {
  await page.goto("http://127.0.0.1:8080");
  await page.setContent("");

  const result = await runWorker(page, validCompilerInput);
  
  expect(typeof result).toBe('string');
  let output = JSON.parse(result);
  expect(output).toHaveProperty('contracts');
  expect(output.contracts['MyContract.sol']).toHaveProperty('MyContract');
  expect(output.contracts['MyContract.sol'].MyContract).toHaveProperty('abi');
  expect(output.contracts['MyContract.sol'].MyContract).toHaveProperty('evm');
  expect(output.contracts['MyContract.sol'].MyContract.evm).toHaveProperty('bytecode');
});
