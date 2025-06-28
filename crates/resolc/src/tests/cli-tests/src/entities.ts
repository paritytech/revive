import * as path from 'path'

const outputDir = 'artifacts'
const binExtension = ':C.pvm'
const asmExtension = ':C.pvmasm'
const llvmExtension = '.ll'
const contractSolFilename = 'contract.sol'
const contractYulFilename = 'contract.yul'
const contractOptimizedLLVMFilename = contractSolFilename + '.C.optimized'
const contractUnoptimizedLLVMFilename = contractSolFilename + '.C.unoptimized'
const pathToOutputDir = path.join(__dirname, '..', outputDir)
const pathToContracts = path.join(__dirname, '..', 'src', 'contracts')
const pathToBasicYulContract = path.join(
  pathToContracts,
  'yul',
  contractYulFilename
)
const pathToMemsetYulContract = path.join(
  pathToContracts,
  'yul',
  'memset.yul'
)
const pathToBasicSolContract = path.join(
  pathToContracts,
  'solidity',
  contractSolFilename
)
const pathToSolBinOutputFile = path.join(
  pathToOutputDir,
  contractSolFilename + binExtension
)
const pathToSolAsmOutputFile = path.join(
  pathToOutputDir,
  contractSolFilename + asmExtension
)

export const paths = {
  outputDir: outputDir,
  binExtension: binExtension,
  asmExtension: asmExtension,
  llvmExtension: llvmExtension,
  contractSolFilename: contractSolFilename,
  contractYulFilename: contractYulFilename,
  contractOptimizedLLVMFilename: contractOptimizedLLVMFilename,
  contractUnoptimizedLLVMFilename: contractUnoptimizedLLVMFilename,
  pathToOutputDir: pathToOutputDir,
  pathToContracts: pathToContracts,
  pathToBasicSolContract: pathToBasicSolContract,
  pathToBasicYulContract: pathToBasicYulContract,
  pathToMemsetYulContract: pathToMemsetYulContract,
  pathToSolBinOutputFile: pathToSolBinOutputFile,
  pathToSolAsmOutputFile: pathToSolAsmOutputFile,
}
