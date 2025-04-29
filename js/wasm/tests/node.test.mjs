import { expect } from "chai";
import { compile } from "../examples/node/revive.js";
import { fileURLToPath } from "url";
import path from "path";
import fs from "fs";
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function loadFixture(fixture) {
  const fixturePath = path.resolve(__dirname, `../fixtures/${fixture}`);
  return JSON.parse(fs.readFileSync(fixturePath, "utf-8"));
}

describe("Compile Function Tests", function () {
  it("should successfully compile valid Solidity code", async function () {
    const standardInput = loadFixture("storage.json");

    const result = await compile(standardInput);
    expect(result).to.be.a("string");
    const output = JSON.parse(result);
    expect(output).to.have.property("contracts");
    expect(output.contracts["fixtures/storage.sol"]).to.have.property(
      "Storage",
    );
    expect(output.contracts["fixtures/storage.sol"].Storage).to.have.property(
      "abi",
    );
    expect(output.contracts["fixtures/storage.sol"].Storage).to.have.property(
      "evm",
    );
    expect(
      output.contracts["fixtures/storage.sol"].Storage.evm,
    ).to.have.property("bytecode");
  });

  if (typeof globalThis.Bun == "undefined") {
    // Running this test with Bun on a Linux host causes:
    // RuntimeError: Out of bounds memory access (evaluating 'getWasmTableEntry(index)(a1, a2, a3, a4, a5)')
    // Once this issue is resolved, the test will be re-enabled.
    it("should successfully compile large Solidity code", async function () {
      const standardInput = loadFixture("token.json");

      const result = await compile(standardInput);
      expect(result).to.be.a("string");
      const output = JSON.parse(result);
      expect(output).to.have.property("contracts");
      expect(output.contracts["fixtures/token.sol"]).to.have.property(
        "MyToken",
      );
      expect(output.contracts["fixtures/token.sol"].MyToken).to.have.property(
        "abi",
      );
      expect(output.contracts["fixtures/token.sol"].MyToken).to.have.property(
        "evm",
      );
      expect(
        output.contracts["fixtures/token.sol"].MyToken.evm,
      ).to.have.property("bytecode");
    });

    it("should successfully compile a valid Solidity contract that instantiates the token contracts", async function () {
      const standardInput = loadFixture("instantiate_tokens.json");

      const result = await compile(standardInput);
      expect(result).to.be.a("string");
      const output = JSON.parse(result);
      expect(output).to.have.property("contracts");
      expect(
        output.contracts["fixtures/instantiate_tokens.sol"],
      ).to.have.property("TokensFactory");
      expect(
        output.contracts["fixtures/instantiate_tokens.sol"].TokensFactory,
      ).to.have.property("abi");
      expect(
        output.contracts["fixtures/instantiate_tokens.sol"].TokensFactory,
      ).to.have.property("evm");
      expect(
        output.contracts["fixtures/instantiate_tokens.sol"].TokensFactory.evm,
      ).to.have.property("bytecode");
    });
  }

  it("should throw an error for invalid Solidity code", async function () {
    const standardInput = loadFixture("invalid_contract_content.json");

    const result = await compile(standardInput);
    expect(result).to.be.a("string");
    const output = JSON.parse(result);
    expect(output).to.have.property("errors");
    expect(output.errors).to.be.an("array");
    expect(output.errors.length).to.be.greaterThan(0);
    expect(output.errors[0].type).to.exist;
    expect(output.errors[0].type).to.contain("ParserError");
  });

  it("should return not found error for missing imports", async function () {
    const standardInput = loadFixture("missing_import.json");

    const result = await compile(standardInput);
    const output = JSON.parse(result);
    expect(output).to.have.property("errors");
    expect(output.errors).to.be.an("array");
    expect(output.errors.length).to.be.greaterThan(0);
    expect(output.errors[0].message).to.exist;
    expect(output.errors[0].message).to.include(
      'Source "nonexistent/console.sol" not found',
    );
  });

  it("should successfully compile a valid Solidity contract that instantiates another contract", async function () {
    const standardInput = loadFixture("instantiate.json");

    const result = await compile(standardInput);
    expect(result).to.be.a("string");
    const output = JSON.parse(result);
    expect(output).to.have.property("contracts");
    expect(output.contracts["fixtures/instantiate.sol"]).to.have.property(
      "ChildContract",
    );
    expect(
      output.contracts["fixtures/instantiate.sol"].ChildContract,
    ).to.have.property("abi");
    expect(
      output.contracts["fixtures/instantiate.sol"].ChildContract,
    ).to.have.property("evm");
    expect(
      output.contracts["fixtures/instantiate.sol"].ChildContract.evm,
    ).to.have.property("bytecode");
    expect(output.contracts["fixtures/instantiate.sol"]).to.have.property(
      "MainContract",
    );
    expect(
      output.contracts["fixtures/instantiate.sol"].MainContract,
    ).to.have.property("abi");
    expect(
      output.contracts["fixtures/instantiate.sol"].MainContract,
    ).to.have.property("evm");
    expect(
      output.contracts["fixtures/instantiate.sol"].MainContract.evm,
    ).to.have.property("bytecode");
  });
});
