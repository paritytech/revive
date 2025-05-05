const { test, expect } = require("@playwright/test");
const fs = require("fs");
const path = require("path");

function loadFixture(fixture) {
  const fixturePath = path.resolve(__dirname, `../fixtures/${fixture}`);
  return JSON.parse(fs.readFileSync(fixturePath, "utf-8"));
}

async function loadTestPage(page) {
  await page.goto("http://127.0.0.1:8080");
  const outputElement = page.locator("#output");
  await outputElement.waitFor({ state: "visible" });
  await page.setContent("");
}

async function runWorker(page, input) {
  return await page.evaluate((input) => {
    return new Promise((resolve, reject) => {
      const worker = new Worker("worker.js");
      worker.postMessage(JSON.stringify(input));

      worker.onmessage = (event) => {
        resolve(event.data.output);
        worker.terminate();
      };

      worker.onerror = (error) => {
        reject(error.message || error);
        worker.terminate();
      };
    });
  }, input);
}

test("should successfully compile valid Solidity code in browser", async ({
  page,
}) => {
  await loadTestPage(page);
  const standardInput = loadFixture("storage.json");
  const result = await runWorker(page, standardInput);

  expect(typeof result).toBe("string");
  let output = JSON.parse(result);
  expect(output).toHaveProperty("contracts");
  expect(output.contracts["fixtures/storage.sol"]).toHaveProperty("Storage");
  expect(output.contracts["fixtures/storage.sol"].Storage).toHaveProperty(
    "abi",
  );
  expect(output.contracts["fixtures/storage.sol"].Storage).toHaveProperty(
    "evm",
  );
  expect(output.contracts["fixtures/storage.sol"].Storage.evm).toHaveProperty(
    "bytecode",
  );
});

test("should successfully compile large valid Solidity code in browser", async ({
  page,
  browserName,
}) => {
  if (browserName === "firefox") {
    // Skipping tests with large contracts on Firefox due to out-of-memory issues.
    test.skip();
  }
  await loadTestPage(page);
  const standardInput = loadFixture("token.json");
  const result = await runWorker(page, standardInput);

  expect(typeof result).toBe("string");
  let output = JSON.parse(result);
  expect(output).toHaveProperty("contracts");
  expect(output.contracts["fixtures/token.sol"]).toHaveProperty("MyToken");
  expect(output.contracts["fixtures/token.sol"].MyToken).toHaveProperty("abi");
  expect(output.contracts["fixtures/token.sol"].MyToken).toHaveProperty("evm");
  expect(output.contracts["fixtures/token.sol"].MyToken.evm).toHaveProperty(
    "bytecode",
  );
});

test("should throw an error for invalid Solidity code in browser", async ({
  page,
}) => {
  await loadTestPage(page);
  const standardInput = loadFixture("invalid_contract_content.json");
  const result = await runWorker(page, standardInput);

  expect(typeof result).toBe("string");
  let output = JSON.parse(result);
  expect(output).toHaveProperty("errors");
  expect(Array.isArray(output.errors)).toBeTruthy(); // Check if it's an array
  expect(output.errors.length).toBeGreaterThan(0);
  expect(output.errors[0]).toHaveProperty("type");
  expect(output.errors[0].type).toContain("ParserError");
});

test("should return not found error for missing imports in browser", async ({
  page,
}) => {
  await loadTestPage(page);
  const standardInput = loadFixture("missing_import.json");
  const result = await runWorker(page, standardInput);

  expect(typeof result).toBe("string");
  let output = JSON.parse(result);
  expect(output).toHaveProperty("errors");
  expect(Array.isArray(output.errors)).toBeTruthy(); // Check if it's an array
  expect(output.errors.length).toBeGreaterThan(0);
  expect(output.errors[0]).toHaveProperty("message");
  expect(output.errors[0].message).toContain(
    'Source "nonexistent/console.sol" not found',
  );
});

test('should successfully compile a valid Solidity contract that instantiates another contract in the browser', async ({ page }) => {
  await loadTestPage(page);
  const standardInput = loadFixture('instantiate.json')
  const result = await runWorker(page, standardInput);
  
  expect(typeof result).toBe('string');
  let output = JSON.parse(result);
  expect(output).toHaveProperty('contracts');
  expect(output.contracts['fixtures/instantiate.sol']).toHaveProperty('ChildContract');
  expect(output.contracts['fixtures/instantiate.sol'].ChildContract).toHaveProperty('abi');
  expect(output.contracts['fixtures/instantiate.sol'].ChildContract).toHaveProperty('evm');
  expect(output.contracts['fixtures/instantiate.sol'].ChildContract.evm).toHaveProperty('bytecode');
  expect(output.contracts['fixtures/instantiate.sol']).toHaveProperty('MainContract');
  expect(output.contracts['fixtures/instantiate.sol'].MainContract).toHaveProperty('abi');
  expect(output.contracts['fixtures/instantiate.sol'].MainContract).toHaveProperty('evm');
  expect(output.contracts['fixtures/instantiate.sol'].MainContract.evm).toHaveProperty('bytecode');
});

test('should successfully compile a valid Solidity contract that instantiates the token contracts in the browser', async ({
  page,
  browserName,
}) => {
  if (browserName === "firefox") {
    // Skipping tests with large contracts on Firefox due to out-of-memory issues.
    test.skip();
  }
  await loadTestPage(page);
  const standardInput = loadFixture('instantiate_tokens.json')
  const result = await runWorker(page, standardInput);

  expect(typeof result).toBe('string');
  let output = JSON.parse(result);
  expect(output).toHaveProperty('contracts');
  expect(output.contracts['fixtures/instantiate_tokens.sol']).toHaveProperty('TokensFactory');
  expect(output.contracts['fixtures/instantiate_tokens.sol'].TokensFactory).toHaveProperty('abi');
  expect(output.contracts['fixtures/instantiate_tokens.sol'].TokensFactory).toHaveProperty('evm');
  expect(output.contracts['fixtures/instantiate_tokens.sol'].TokensFactory.evm).toHaveProperty('bytecode');
});
