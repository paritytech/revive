mergeInto(LibraryManager.library, {
  soljson_compile: function (inputPtr, inputLen) {
    const inputJson = UTF8ToString(inputPtr, inputLen);
    const output = Module.soljson.cwrap("solidity_compile", "string", [
      "string",
    ])(inputJson);
    return stringToNewUTF8(output);
  },
  soljson_version: function () {
    const version = Module.soljson.cwrap("solidity_version", "string", [])();
    return stringToNewUTF8(version);
  },
  resolc_compile: function (inputPtr, inputLen) {
    const inputJson = UTF8ToString(inputPtr, inputLen);
    var revive = createRevive();
    // Allow GC to clean up the data
    revive.wasmBinary = undefined
    revive.soljson = undefined
    revive.writeToStdin(inputJson);

    // Call main on the new instance
    const result = revive.callMain(["--recursive-process"]);

    if (result) {
      const stderrString = revive.readFromStderr();
      const error = JSON.stringify({
        type: "error",
        message: stderrString || "Unknown error",
      });
      return stringToNewUTF8(error);
    } else {
      const stdoutString = revive.readFromStdout();
      const json = JSON.stringify({ type: "success", data: stdoutString });
      return stringToNewUTF8(json);
    }
  },
});
