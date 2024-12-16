
importScripts('./soljson.js');
importScripts('./resolc.js');

// Handle messages from the main thread
onmessage = async function (e) {
  const contractCode = e.data.contractCode
  const sourceCode = {
      language: 'Solidity',
      sources: {
          contract: {
              content: contractCode,
          }
      },
      settings: {
          optimizer: {
            enabled: true,
            runs: 200,
          },
          outputSelection: {
              '*': {
                '*': ['abi'],
            }
          }
      }
  };
    const m = createRevive();

    m.soljson = Module;

    // Set input data for stdin
    m.writeToStdin(JSON.stringify(sourceCode));

    // Compile the Solidity source code
    m.callMain(['--standard-json']);

  postMessage({output: m.readFromStdout() || m.readFromStderr()});
};
