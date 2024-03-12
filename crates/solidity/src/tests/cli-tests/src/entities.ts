import * as path from 'path';

const outputDir = 'artifacts';
const binExtension = ':C.zbin';
const asmExtension = ':C.zasm';
const contractSolFilename = 'contract.sol';
const contractYulFilename = 'contract.yul';
const contractZkasmFilename = 'contract.zkasm';
const pathToOutputDir = path.join( __dirname, '..', outputDir);
const pathToContracts = path.join( __dirname, '..', 'src', 'contracts');
const pathToBasicYulContract = path.join(pathToContracts, 'yul', contractYulFilename);
const pathToBasicZkasmContract = path.join(pathToContracts, 'zkasm', contractZkasmFilename);
const pathToBasicSolContract = path.join(pathToContracts, 'solidity', contractSolFilename);
const pathToSolBinOutputFile = path.join(pathToOutputDir, contractSolFilename + binExtension);
const pathToSolAsmOutputFile = path.join(pathToOutputDir, contractSolFilename + asmExtension);

export const paths = {
  outputDir: outputDir,
  binExtension: binExtension,
  asmExtension: asmExtension,
  contractSolFilename: contractSolFilename,
  contractZkasmFilename: contractZkasmFilename,
  contractYulFilename: contractYulFilename,
  pathToOutputDir: pathToOutputDir,
  pathToContracts: pathToContracts,
  pathToBasicZkasmContract: pathToBasicZkasmContract,
  pathToBasicSolContract: pathToBasicSolContract,
  pathToBasicYulContract: pathToBasicYulContract,
  pathToSolBinOutputFile: pathToSolBinOutputFile,
  pathToSolAsmOutputFile: pathToSolAsmOutputFile,
};
