import soljson from 'solc/soljson'
import Resolc from './resolc/resolc'
import type { SolcOutput } from '.'

export async function resolc(input: string): Promise<SolcOutput> {
  const m = (await Resolc()) as any // eslint-disable-line @typescript-eslint/no-explicit-any
  m.soljson = soljson
  m.writeToStdin(input)
  m.callMain(['--standard-json'])
  const err = m.readFromStderr()

  if (err) {
    throw new Error(err)
  }

  return JSON.parse(m.readFromStdout()) as SolcOutput
}

export async function version(): Promise<string> {
  const m = (await Resolc()) as any // eslint-disable-line @typescript-eslint/no-explicit-any
  m.soljson = soljson
  m.callMain(['--version'])
  const err = m.readFromStderr()

  if (err) {
    throw new Error(err)
  }

  return m.readFromStdout()
}
