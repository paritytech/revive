import soljson from 'solc/soljson'
import Resolc from './resolc/resolc'
import type { SolcOutput } from '.'

export function resolc(input: string): SolcOutput {
    const m = Resolc() as any // eslint-disable-line @typescript-eslint/no-explicit-any
    m.soljson = soljson
    m.writeToStdin(input)
    m.callMain(['--standard-json'])
    const err = m.readFromStderr()

    if (err) {
        throw new Error(err)
    }

    return JSON.parse(m.readFromStdout()) as SolcOutput
}

export function version(): string {
    const m = Resolc() as any // eslint-disable-line @typescript-eslint/no-explicit-any
    m.soljson = soljson
    m.callMain(['--version'])
    const err = m.readFromStderr()

    if (err) {
        throw new Error(err)
    }

    return m.readFromStdout()
}
