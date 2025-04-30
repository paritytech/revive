Module.stdinData = null;
Module.stdinDataPosition = 0;
Module.stdoutData = [];
Module.stderrData = [];

// Method to read all collected stdout data
Module.readFromStdout = function () {
  if (!Module.stdoutData.length) return "";
  const decoder = new TextDecoder("utf-8");
  const data = decoder.decode(new Uint8Array(Module.stdoutData));
  Module.stdoutData = [];
  return data;
};

// Method to read all collected stderr data
Module.readFromStderr = function () {
  if (!Module.stderrData.length) return "";
  const decoder = new TextDecoder("utf-8");
  const data = decoder.decode(new Uint8Array(Module.stderrData));
  Module.stderrData = [];
  return data;
};

// Method to write data to stdin
Module.writeToStdin = function (data) {
  const encoder = new TextEncoder();
  Module.stdinData = encoder.encode(data);
  Module.stdinDataPosition = 0;
};

// Override the `preRun` method to customize file system initialization
Module.preRun = Module.preRun || [];
Module.preRun.push(function () {
  // Custom stdin function
  function customStdin() {
    if (
      !Module.stdinData ||
      Module.stdinDataPosition >= Module.stdinData.length
    ) {
      return null; // End of input (EOF)
    }
    return Module.stdinData[Module.stdinDataPosition++];
  }

  // Custom stdout function
  function customStdout(char) {
    Module.stdoutData.push(char);
  }

  // Custom stderr function
  function customStderr(char) {
    Module.stderrData.push(char);
  }

  // Initialize the FS (File System) with custom handlers
  FS.init(customStdin, customStdout, customStderr);
});
