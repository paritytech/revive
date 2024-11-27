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
        const path = require('path');
        const createRevive = require(path.resolve(__dirname, './resolc.js'));
        const revive = createRevive();

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
