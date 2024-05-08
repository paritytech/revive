import * as path from 'path';

const outputDir = 'artifacts';
const binExtension = ':C.pvm';
const asmExtension = ':C.pvmasm';
const contractSolFilename = 'contract.sol';
const contractYulFilename = 'contract.yul';
const pathToOutputDir = path.join(__dirname, '..', outputDir);
const pathToContracts = path.join(__dirname, '..', 'src', 'contracts');
const pathToBasicYulContract = path.join(pathToContracts, 'yul', contractYulFilename);
const pathToBasicSolContract = path.join(pathToContracts, 'solidity', contractSolFilename);
const pathToSolBinOutputFile = path.join(pathToOutputDir, contractSolFilename + binExtension);
const pathToSolAsmOutputFile = path.join(pathToOutputDir, contractSolFilename + asmExtension);

export const paths = {
  outputDir: outputDir,
  binExtension: binExtension,
  asmExtension: asmExtension,
  contractSolFilename: contractSolFilename,
  contractYulFilename: contractYulFilename,
  pathToOutputDir: pathToOutputDir,
  pathToContracts: pathToContracts,
  pathToBasicSolContract: pathToBasicSolContract,
  pathToBasicYulContract: pathToBasicYulContract,
  pathToSolBinOutputFile: pathToSolBinOutputFile,
  pathToSolAsmOutputFile: pathToSolAsmOutputFile,
};
