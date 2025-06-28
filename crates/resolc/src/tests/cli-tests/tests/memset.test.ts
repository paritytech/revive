import { executeCommand } from '../src/helper'
import { paths } from '../src/entities'

describe('tests for the memset builtin to be present', () => {
  // -O3 is required to reproduce.
  const command = `resolc ${paths.pathToMemsetYulContract} --yul -O3`
  const result = executeCommand(command)

  it('Valid command exit code = 0', () => {
    expect(result.exitCode).toBe(0)
  })

  it('--yul output is presented', () => {
    expect(result.output).toMatch(/(Compiler run successful)/i)
    expect(result.output).toMatch(/(No output requested)/i)
  })

})
