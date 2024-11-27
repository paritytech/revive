mergeInto(LibraryManager.library, {
    soljson_compile: function(inputPtr, inputLen) {
        const inputJson = UTF8ToString(inputPtr, inputLen);
        const output = Module.soljson.cwrap('solidity_compile', 'string', ['string'])(inputJson);
        return stringToNewUTF8(output);
    },
    soljson_version: function() {
        const version = Module.soljson.cwrap("solidity_version", "string", [])();
        return stringToNewUTF8(version);
    },
    resolc_compile: function(inputPtr, inputLen) {
        const inputJson = UTF8ToString(inputPtr, inputLen);

        // Check if running in a web worker or node.js
        if (typeof importScripts === 'function') {
            // Running in a web worker
            importScripts('./resolc.js');
            var revive = createRevive()
        } else if (typeof require === 'function') {
            // Running in Node.js
            const path = require('path');
            createRevive = require(path.resolve(__dirname, './resolc.js'));  // `createRevive` is returned from the required module
            var revive = createRevive();
        } else {
            throw new Error('Unknown environment: Unable to load resolc.js');
        }
        revive.setStdinData(inputJson);

        let stdoutString = "";
        revive.setStdoutCallback(function(char) {
            if (char.charCodeAt(0) === '\n') {
                exit;
            }
            stdoutString += char;
        });

        let stderrString = "";
        revive.setStderrCallback(function(char) {
            stderrString += char;
        });

        // Call main on the new instance
        const result = revive.callMain(['--recursive-process']);

        if (result) {
            const error = JSON.stringify({ type: 'error', message: stderrString || "Unknown error" });
            return stringToNewUTF8(error);
        } else {
            const json = JSON.stringify({ type: 'success', data: stdoutString });
            return stringToNewUTF8(json);
        }
    },
});
