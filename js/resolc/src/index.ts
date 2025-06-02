import solc from 'solc'
import { spawn } from 'child_process'
import { resolc, version as resolcVersion } from './resolc'
import path from 'path'
import { existsSync, readFileSync } from 'fs'
import resolvePkg from 'resolve-pkg'

export type SolcInput = {
  [contractName: string]: {
    content: string
  }
}

export type SolcError = {
  component: string
  errorCode: string
  formattedMessage: string
  message: string
  severity: string
  sourceLocation?: {
    file: string
    start: number
    end: number
  }
  type: string
}

export type SolcOutput = {
  contracts: {
    [contractPath: string]: {
      [contractName: string]: {
        abi: Array<{
          name: string
          inputs: Array<{ name: string; type: string }>
          outputs: Array<{ name: string; type: string }>
          stateMutability: string
          type: string
        }>
        evm: {
          bytecode: { object: string }
        }
      }
    }
  }
  errors?: Array<SolcError>
}

export function resolveInputs(sources: SolcInput): SolcInput {
  const input = {
    language: 'Solidity',
    sources,
    settings: {
      outputSelection: {
        '*': {
          '*': ['evm.bytecode.object'],
        },
      },
    },
  }

  const out = solc.compile(JSON.stringify(input), {
    import: (path: string) => {
      return {
        contents: readFileSync(tryResolveImport(path), 'utf8'),
      }
    },
  })

  const output = JSON.parse(out) as {
    sources: { [fileName: string]: { id: number } }
    errors: Array<SolcError>
  }

  if (output.errors && Object.keys(output.sources).length === 0) {
    throw new Error(output.errors[0].formattedMessage)
  }

  return Object.fromEntries(
    Object.keys(output.sources).map((fileName) => {
      return [
        fileName,
        sources[fileName] ?? {
          content: readFileSync(tryResolveImport(fileName), 'utf8'),
        },
      ]
    })
  )
}

export function version(): string {
  const v = resolcVersion()
  return v.split(' ').pop() ?? v
}

export async function compile(
  sources: SolcInput,
  option: {
    optimizer?: Record<string, unknown>
    bin?: string
  } = {}
): Promise<SolcOutput> {
  const {
    optimizer = {
      mode: 'z',
      fallback_to_optimizing_for_size: true,
      enabled: true,
      runs: 200,
    },
    bin,
  } = option

  const input = JSON.stringify({
    language: 'Solidity',
    sources: resolveInputs(sources),
    settings: {
      optimizer,
      outputSelection: {
        '*': {
          '*': ['abi'],
        },
      },
    },
  })

  if (bin) {
    return compileWithBin(input, bin)
  }

  return resolc(input)
}

/**
 * Resolve an import path to a file path.
 * @param importPath - The import path to resolve.
 */
export function tryResolveImport(importPath: string) {
  // resolve local path
  if (existsSync(importPath)) {
    return path.resolve(importPath)
  }

  const importRegex = /^(@?[^@/]+(?:\/[^@/]+)?)(?:@([^/]+))?(\/.+)$/
  const match = importPath.match(importRegex)

  if (!match) {
    throw new Error('Invalid import path format.')
  }

  const basePackage = match[1] // "foo", "@scope/foo"
  const specifiedVersion = match[2] // "1.2.3" (optional)
  const relativePath = match[3] // "/path/to/file.sol"

  const packageRoot = resolvePkg(basePackage)
  if (!packageRoot) {
    throw new Error(`Package ${basePackage} not found.`)
  }

  // Check if a version was specified and compare with the installed version
  if (specifiedVersion) {
    const installedVersion = JSON.parse(
      readFileSync(path.join(packageRoot, 'package.json'), 'utf-8')
    ).version

    if (installedVersion !== specifiedVersion) {
      throw new Error(
        `Version mismatch: Specified ${basePackage}@${specifiedVersion}, but installed version is ${installedVersion}`
      )
    }
  }

  // Construct full path to the requested file
  const resolvedPath = path.join(packageRoot, relativePath)
  if (existsSync(resolvedPath)) {
    return resolvedPath
  } else {
    throw new Error(`Resolved path ${resolvedPath} does not exist.`)
  }
}
function compileWithBin(input: string, bin: string): PromiseLike<SolcOutput> {
  return new Promise((resolve, reject) => {
    const process = spawn(bin, ['--standard-json'])

    let output = ''
    let error = ''

    process.stdin.write(input)
    process.stdin.end()

    process.stdout.on('data', (data) => {
      output += data.toString()
    })

    process.stderr.on('data', (data) => {
      error += data.toString()
    })

    process.on('close', (code) => {
      if (code === 0) {
        try {
          const result: SolcOutput = JSON.parse(output)
          resolve(result)
        } catch {
          reject(new Error(`Failed to parse output`))
        }
      } else {
        reject(new Error(`Process exited with code ${code}: ${error}`))
      }
    })
  })
}
