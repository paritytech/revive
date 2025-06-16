import { test } from 'node:test'
import { readFileSync, existsSync } from 'node:fs'
import assert from 'node:assert'
import { compile, tryResolveImport } from '.'
import { resolve } from 'node:path'

const compileOptions = [{}]
if (existsSync('../../target/release/resolc')) {
  compileOptions.push({ bin: '../../target/release/resolc' })
}

for (const options of compileOptions) {
  test(`check Ok output with option ${JSON.stringify(options)}`, async () => {
    const contract = 'fixtures/token.sol'
    const sources = {
      [contract]: {
        content: readFileSync('fixtures/storage.sol', 'utf8'),
      },
    }

    const out = await compile(sources, options)
    assert(out.contracts[contract].Storage.abi != null)
    assert(out.contracts[contract].Storage.evm.bytecode != null)
  })
}

test('check Err output', async () => {
  const sources = {
    bad: {
      content: readFileSync('fixtures/storage_bad.sol', 'utf8'),
    },
  }

  const out = await compile(sources)
  assert(
    out?.errors?.[0].message.includes(
      'SPDX license identifier not provided in source file'
    )
  )
  assert(
    out?.errors?.[1].message.includes(
      'Source file does not specify required compiler version'
    )
  )
})

test('check Err from stderr', async () => {
  const sources = {
    bad: {
      content: readFileSync('fixtures/bad_pragma.sol', 'utf8'),
    },
  }

  try {
    await compile(sources)
    assert(false, 'Expected error')
  } catch (error) {
    assert(
      String(error).includes('Source file requires different compiler version')
    )
  }
})

test('resolve import', () => {
  const cases = [
    // local
    {
      file: './fixtures/storage.sol',
      expected: resolve('fixtures/storage.sol'),
    },

    // package with exports
    {
      file: '@redstone-finance/evm-connector/contracts/data-services/PrimaryProdDataServiceConsumerBase.sol',
      expected: resolve(
        __dirname,
        '../../..',
        'node_modules/@redstone-finance/evm-connector/contracts/data-services/PrimaryProdDataServiceConsumerBase.sol'
      ),
    },
    // scopped module with version
    {
      file: '@openzeppelin/contracts@5.1.0/token/ERC20/ERC20.sol',
      expected: require.resolve(
        '@openzeppelin/contracts/token/ERC20/ERC20.sol'
      ),
    },
    // scopped module without version
    {
      file: '@openzeppelin/contracts/token/ERC20/ERC20.sol',
      expected: require.resolve(
        '@openzeppelin/contracts/token/ERC20/ERC20.sol'
      ),
    },
    // scopped module with wrong version
    {
      file: '@openzeppelin/contracts@4.8.3/token/ERC20/ERC20.sol',
      expected: `Error: Version mismatch: Specified @openzeppelin/contracts@4.8.3, but installed version is 5.1.0`,
    },
    // module without version
    {
      file: '@openzeppelin/contracts/package.json',
      expected: require.resolve('@openzeppelin/contracts/package.json'),
    },
    // scopped module with version
    {
      file: '@openzeppelin/contracts@5.1.0/package.json',
      expected: require.resolve('@openzeppelin/contracts/package.json'),
    },
  ]

  for (const { file, expected } of cases) {
    let resolved
    try {
      resolved = tryResolveImport(file)
    } catch (error) {
      assert(
        String(error) == expected,
        `\nExpected:\n${expected}\nGot:\n${String(error)}\n`
      )
    }
    if (resolved) {
      assert(
        resolved === expected,
        `\nExpected:\n${expected}\nGot:\n${resolved}`
      )
    }
  }
})
