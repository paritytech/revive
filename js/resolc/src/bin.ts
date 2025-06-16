#!/usr/bin/env node

import * as commander from 'commander'
import * as fs from 'fs'
import * as os from 'os'
import * as path from 'path'
import * as resolc from '.'
import { SolcInput } from '.'
import { execSync } from 'child_process'

async function main() {
  // hold on to any exception handlers that existed prior to this script running, we'll be adding them back at the end
  const originalUncaughtExceptionListeners =
    process.listeners('uncaughtException')
  // FIXME: remove annoying exception catcher of Emscripten
  //        see https://github.com/chriseth/browser-solidity/issues/167
  process.removeAllListeners('uncaughtException')

  const program = new commander.Command()

  program.name('resolcjs')
  program.version(resolc.version())
  program
    .option('--bin', 'Binary of the contracts in hex.')
    .option('--abi', 'ABI of the contracts.')
    .option('--stats', 'Print statistics about Resolc vs Solc compilation.')
    .option(
      '--base-path <path>',
      'Root of the project source tree. ' +
        'The import callback will attempt to interpret all import paths as relative to this directory.'
    )
    .option(
      '--include-path <path...>',
      'Extra source directories available to the import callback. ' +
        'When using a package manager to install libraries, use this option to specify directories where packages are installed. ' +
        'Can be used multiple times to provide multiple locations.'
    )
    .option(
      '-o, --output-dir <output-directory>',
      'Output directory for the contracts.'
    )
    .option('-p, --pretty-json', 'Pretty-print all JSON output.', false)
    .option('-v, --verbose', 'More detailed console output.', false)
    .argument('<files...>')

  program.parse(process.argv)
  const options = program.opts<{
    verbose: boolean
    abi: boolean
    bin: boolean
    outputDir?: string
    prettyJson: boolean
    basePath?: string
    stats: boolean
    includePath?: string[]
  }>()

  // when using --stats option, we want to run solc as well to compare outputs size
  if (options.stats) {
    const args = process.argv.filter((arg) => !arg.startsWith('--stats'))
    try {
      execSync(`npx solc ${args.slice(2).join(' ')}`)
    } catch (err) {
      abort(`Failed to run solc: ${err}`)
    }
  }

  const files: string[] = program.args
  const destination = options.outputDir ?? '.'

  function abort(msg: string) {
    console.error(msg || 'Error occurred')
    process.exit(1)
  }

  function withUnixPathSeparators(filePath: string) {
    // On UNIX-like systems forward slashes in paths are just a part of the file name.
    if (os.platform() !== 'win32') {
      return filePath
    }

    return filePath.replace(/\\/g, '/')
  }

  function makeSourcePathRelativeIfPossible(sourcePath: string) {
    const absoluteBasePath = options.basePath
      ? path.resolve(options.basePath)
      : path.resolve('.')
    const absoluteIncludePaths = options.includePath
      ? options.includePath.map((prefix: string) => {
          return path.resolve(prefix)
        })
      : []

    // Compared to base path stripping logic in solc this is much simpler because path.resolve()
    // handles symlinks correctly (does not resolve them except in work dir) and strips .. segments
    // from paths going beyond root (e.g. `/../../a/b/c` -> `/a/b/c/`). It's simpler also because it
    // ignores less important corner cases: drive letters are not stripped from absolute paths on
    // Windows and UNC paths are not handled in a special way (at least on Linux). Finally, it has
    // very little test coverage so there might be more differences that we are just not aware of.
    const absoluteSourcePath = path.resolve(sourcePath)

    for (const absolutePrefix of [absoluteBasePath].concat(
      absoluteIncludePaths
    )) {
      const relativeSourcePath = path.relative(
        absolutePrefix,
        absoluteSourcePath
      )

      if (!relativeSourcePath.startsWith('../')) {
        return withUnixPathSeparators(relativeSourcePath)
      }
    }

    // File is not located inside base path or include paths so use its absolute path.
    return withUnixPathSeparators(absoluteSourcePath)
  }

  function toFormattedJson<T>(input: T) {
    return JSON.stringify(input, null, options.prettyJson ? 4 : 0)
  }

  if (files.length === 0) {
    console.error('Must provide a file')
    process.exit(1)
  }

  if (!(options.bin || options.abi)) {
    abort('Invalid option selected, must specify either --bin or --abi')
  }

  const sources: SolcInput = {}

  for (let i = 0; i < files.length; i++) {
    try {
      sources[makeSourcePathRelativeIfPossible(files[i])] = {
        content: fs.readFileSync(files[i]).toString(),
      }
    } catch (e) {
      abort('Error reading ' + files[i] + ': ' + e)
    }
  }

  if (options.verbose) {
    console.log('>>> Compiling:\n' + toFormattedJson(sources) + '\n')
  }

  const output = await resolc.compile(sources)
  let hasError = false

  if (!output) {
    abort('No output from compiler')
  } else if (output.errors) {
    for (const error in output.errors) {
      const message = output.errors[error]
      if (message.severity === 'warning') {
        console.log(message.formattedMessage)
      } else {
        console.error(message.formattedMessage)
        hasError = true
      }
    }
  }

  fs.mkdirSync(destination, { recursive: true })

  function writeFile(file: string, content: Buffer | string) {
    file = path.join(destination, file)
    fs.writeFile(file, content, function (err) {
      if (err) {
        console.error('Failed to write ' + file + ': ' + err)
      }
    })
  }

  const contractStats = []

  for (const fileName in output.contracts) {
    for (const contractName in output.contracts[fileName]) {
      let contractFileName = fileName + ':' + contractName
      contractFileName = contractFileName.replace(/[:./\\]/g, '_')
      let polkavmSize = 0
      let binSize = 0
      if (
        options.bin &&
        output.contracts?.[fileName]?.[contractName]?.evm?.bytecode?.object
      ) {
        const pvmData = Buffer.from(
          output.contracts[fileName][contractName].evm.bytecode.object,
          'hex'
        )
        writeFile(contractFileName + '.polkavm', pvmData)
        polkavmSize = pvmData.length

        const binOutPath = path.join(destination, `${contractFileName}.bin`)
        if (fs.existsSync(binOutPath)) {
          try {
            binSize = fs.statSync(binOutPath).size || 0
          } catch {}
        }
        contractStats.push({
          file: fileName,
          contract: contractName,
          polkavm: (polkavmSize / 1024).toFixed(2) + ' kB',
          bin: (binSize / 1024).toFixed(2) + ' kB',
          diff:
            binSize > 0
              ? `${((polkavmSize / binSize - 1) * 100).toFixed(2)}%`
              : 'N/A',
        })
      }

      if (options.abi) {
        writeFile(
          contractFileName + '.abi',
          toFormattedJson(output.contracts[fileName][contractName].abi)
        )
      }
    }
  }

  if (options.stats && contractStats.length > 0) {
    console.table(contractStats)
  }

  // Put back original exception handlers.
  originalUncaughtExceptionListeners.forEach(function (listener) {
    process.addListener('uncaughtException', listener)
  })

  if (hasError) {
    process.exit(1)
  }
}

main().catch((err) => {
  console.error('Error:', err)
  process.exit(1)
})
