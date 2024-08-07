// Run with:
//     node --experimental-default-type=module testcase.js
// (for Node version 18.20.3)

import { runResolc } from '../src/tools.js'

test('resolc handles \'--version\' option', async () => {
    const retval = await runResolc(["--version"]);
    expect(retval).toBe(0);
})

test('resolc handles \'--help\' option', async () => {
    const retval = await runResolc(["--help"]);
    expect(retval).toBe(0);

})

test('resolc handles a misspelled option', async () => {
    const retval = await runResolc(["--hlp"]);
    expect(retval).not.toBe(0);
})
