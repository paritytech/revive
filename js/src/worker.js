import { parentPort } from 'worker_threads';

parentPort.on('message', async (inputJson) => {
    const { default: createRevive } = await import(new URL('./resolc.js', import.meta.url));
    const revive = await createRevive();

    revive.setStdinData(inputJson);

    let stdoutString = "";
    revive.setStdoutCallback(function(char) {
        if (char.charCodeAt(0) === '\n') {
            console.log("new line")
            exit
        }
        stdoutString += char;
    });

    let stderrString = "";
    revive.setStderrCallback(function(char) {
        stderrString += char;
    });

    // Call main on the new instance
    const output = revive.callMain(['--recursive-process']);

    if (stderrString.length > 0) {
        // If /err is not empty, throw an error with its content
        throw new Error(stderrString);
    } else {
        parentPort.postMessage({ output: stdoutString });
    }
});
