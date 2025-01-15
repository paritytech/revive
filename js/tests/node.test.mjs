import { expect } from 'chai';
import { compile } from '../examples/node/revive.js';
import { fileURLToPath } from 'url';
import path from 'path';
import fs from 'fs';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function loadFixture(fixture) {
  const fixturePath = path.resolve(__dirname, `../fixtures/${fixture}`);
  return  JSON.parse(fs.readFileSync(fixturePath, 'utf-8'));
}

describe('Compile Function Tests', function () {
  it('should successfully compile valid Solidity code', async function () {
    const standardInput = loadFixture('storage.json')

    const result = await compile(standardInput);
    expect(result).to.be.a('string');
    const output = JSON.parse(result);
    expect(output).to.have.property('contracts');
    expect(output.contracts['fixtures/storage.sol']).to.have.property('Storage');
    expect(output.contracts['fixtures/storage.sol'].Storage).to.have.property('abi');
    expect(output.contracts['fixtures/storage.sol'].Storage).to.have.property('evm');
    expect(output.contracts['fixtures/storage.sol'].Storage.evm).to.have.property('bytecode');
  });

  it('should successfully compile large Solidity code', async function () {
    const standardInput = loadFixture('token.json')

    const result = await compile(standardInput);
    expect(result).to.be.a('string');
    const output = JSON.parse(result);
    expect(output).to.have.property('contracts');
    expect(output.contracts['fixtures/token.sol']).to.have.property('MyToken');
    expect(output.contracts['fixtures/token.sol'].MyToken).to.have.property('abi');
    expect(output.contracts['fixtures/token.sol'].MyToken).to.have.property('evm');
    expect(output.contracts['fixtures/token.sol'].MyToken.evm).to.have.property('bytecode');
  });

  it('should throw an error for invalid Solidity code', async function () {
    const standardInput = loadFixture('invalid_contract_content.json')

    const result = await compile(standardInput);
    expect(result).to.be.a('string');
    const output = JSON.parse(result);
    expect(output).to.have.property('errors');
    expect(output.errors).to.be.an('array');
    expect(output.errors.length).to.be.greaterThan(0);
    expect(output.errors[0].type).to.exist;
    expect(output.errors[0].type).to.contain("ParserError");
  });

  it('should return not found error for missing imports', async function () {
    const standardInput = loadFixture('missing_import.json')

    const result = await compile(standardInput);    
    const output = JSON.parse(result);
    expect(output).to.have.property('errors');
    expect(output.errors).to.be.an('array');
    expect(output.errors.length).to.be.greaterThan(0);
    expect(output.errors[0].message).to.exist;
    expect(output.errors[0].message).to.include('Source "nonexistent/console.sol" not found');
  });
});
