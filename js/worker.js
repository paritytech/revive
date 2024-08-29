// worker.js
// nodejs version
const { parentPort } = require('worker_threads');

parentPort.on('message', async (inputJson) => {
    const { default: ModuleFactory } = await import('./resolc.mjs');
    const newModule = await ModuleFactory();

    // Create a virtual file for stdin
    newModule.FS.writeFile('/in', inputJson);

    // Call main on the new instance
    const output = newModule.callMain(['--recursive-process']);

    // Check the /err file content
    const errorMessage = newModule.FS.readFile('/err', { encoding: 'utf8' });

    if (errorMessage.length > 0) {
        // If /err is not empty, throw an error with its content
        throw new Error(errorMessage);
    } else {
        // If no error, read the output file
        let outputFile = newModule.FS.readFile('/out', { encoding: 'utf8' });
        parentPort.postMessage({ output: outputFile });
    }
});
