mergeInto(LibraryManager.library, {
    soljson_compile: function(inputPtr, inputLen) {
        const inputJson = UTF8ToString(inputPtr, inputLen);
        const output = Module.solc.compile(inputJson)
        return stringToNewUTF8(output)
    },
    soljson_version: function() {
        var version = Module.solc.version();
        return stringToNewUTF8(version)
    },
    resolc_compile: function(inputPtr, inputLen) {
        const { Worker } = require('worker_threads');
        const deasync = require('deasync');

        var inputJson = UTF8ToString(inputPtr, inputLen);

        // Inline worker code
        const workerCode = `
        // worker.js
        // nodejs version
        const { parentPort } = require('worker_threads');

        parentPort.on('message', async (inputJson) => {
            const { default: ModuleFactory } = await import('./resolc.js');
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
        });`;

        function compileWithWorker(inputJson, callback) {
            return new Promise((resolve, reject) => {
                // Create a new Worker
                const worker = new Worker(workerCode, { eval: true });

                // Listen for messages from the worker
                worker.on('message', (message) => {
                    resolve(message.output);  // Resolve the promise with the output
                    callback(null, message.output);
                    worker.terminate(); // Terminate the worker after processing
                });

                // Listen for errors from the worker
                worker.on('error', (error) => {
                    reject(error);
                    callback(error);
                    worker.terminate();
                });

                // Send the input JSON to the worker
                worker.postMessage(inputJson);
            });
        }
        let result = null;
        let error = null;

        // Use deasync to block until promise resolves
        compileWithWorker(inputJson, function (err, res) {
            error = err;
            result = res;
        });
        // TODO: deasync is not present in browsers, another solution needs to be implemented
        deasync.loopWhile(() => result === null && error === null);

        if (error) {
            const errorJson = JSON.stringify({ type: 'error', message: error.message || "Unknown error" });
            return stringToNewUTF8(errorJson)
        }

        const resultJson = JSON.stringify({ type: 'success', data: result });
        return stringToNewUTF8(resultJson);
    },
});
