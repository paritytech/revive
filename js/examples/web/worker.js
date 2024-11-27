
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
    m.setStdinData(JSON.stringify(sourceCode));

    var stdoutString = "";
    m.setStdoutCallback(function(char) {
        if (char.charCodeAt(0) === '\n') {
            console.log("new line")
            exit
        }
        stdoutString += char;
    });

    var stderrString = "";
    m.setStderrCallback(function(char) {
        stderrString += char;
    });

    // Compile the Solidity source code
    m.callMain(['--standard-json']);

  postMessage({output: stdoutString || stderrString});
};
