importScripts("./resolc_web.js");

// Handle messages from the main thread
onmessage = function (e) {
  const m = createRevive();

  // Set input data for stdin
  m.writeToStdin(e.data);

  // Compile the Solidity source code
  m.callMain(["--standard-json"]);

  postMessage({ output: m.readFromStdout() || m.readFromStderr() });
};
