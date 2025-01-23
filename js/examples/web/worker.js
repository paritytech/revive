importScripts("./soljson.js");
importScripts("./resolc.js");

// Handle messages from the main thread
onmessage = async function (e) {
  const m = createRevive();
  m.soljson = Module;

  // Set input data for stdin
  m.writeToStdin(e.data);

  // Compile the Solidity source code
  m.callMain(["--standard-json"]);

  postMessage({ output: m.readFromStdout() || m.readFromStderr() });
};
