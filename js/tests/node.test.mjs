import { expect } from 'chai';
import { compile } from '../examples/node/revive.js';

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

describe('Compile Function Tests', function () {
  it('should successfully compile valid Solidity code', async function () {
    const result = await compile(validCompilerInput);

    // Ensure result contains compiled contract
    expect(result).to.be.a('string');
    const output = JSON.parse(result);
    expect(output).to.have.property('contracts');
    expect(output.contracts['MyContract.sol']).to.have.property('MyContract');
    expect(output.contracts['MyContract.sol'].MyContract).to.have.property('abi');
    expect(output.contracts['MyContract.sol'].MyContract).to.have.property('evm');
    expect(output.contracts['MyContract.sol'].MyContract.evm).to.have.property('bytecode');
  });

  it('should throw an error for invalid Solidity code', async function () {
    const invalidCompilerInput = {
      ...validCompilerInput,
      sources: {
        'MyContract.sol': {
          content: `
            // SPDX-License-Identifier: UNLICENSED
            pragma solidity ^0.8.0; 
            import "nonexistent/console.sol";
            contract MyContract { 
              function greet() public pure returns (string memory) { 
                return "Hello" // Missing semicolon
              } 
            }
          `,
        },
      },
    };

    const result = await compile(invalidCompilerInput);
    expect(result).to.be.a('string');
    const output = JSON.parse(result);
    expect(output).to.have.property('errors');
    expect(output.errors).to.be.an('array');
    expect(output.errors.length).to.be.greaterThan(0);
    expect(output.errors[0].type).to.exist;
    expect(output.errors[0].type).to.contain("ParserError");
  });

  it('should return not found error for missing imports', async function () {
    const compilerInputWithImport = {
      ...validCompilerInput,
      sources: {
        'MyContract.sol': {
          content: `
            // SPDX-License-Identifier: UNLICENSED
            pragma solidity ^0.8.0; 
            import "nonexistent/console.sol";
            contract MyContract { 
              function greet() public pure returns (string memory) { 
                return "Hello"; 
              } 
            }
          `,
        },
      },
    };

    let result = await compile(compilerInputWithImport);
    const output = JSON.parse(result);
    expect(output).to.have.property('errors');
    expect(output.errors).to.be.an('array');
    expect(output.errors.length).to.be.greaterThan(0);
    expect(output.errors[0].message).to.exist;
    expect(output.errors[0].message).to.include('Source "nonexistent/console.sol" not found');
  });
});
