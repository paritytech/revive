const soljson = require("solc/soljson");
const createRevive = require("./resolc.js");

function compile(standardJsonInput) {
  if (!standardJsonInput) {
    throw new Error("Input JSON for the Solidity compiler is required.");
  }

  // Initialize the compiler
  const compiler = createRevive();
  compiler.soljson = soljson;

  // Provide input to the compiler
  compiler.writeToStdin(JSON.stringify(standardJsonInput));

  // Run the compiler
  compiler.callMain(["--standard-json"]);

  // Collect output
  const stdout = compiler.readFromStdout();
  const stderr = compiler.readFromStderr();

  // Check for errors and throw if stderr exists
  if (stderr) {
    throw new Error(`Compilation failed: ${stderr}`);
  }

  // Return the output if no errors
  return stdout;
}

module.exports = { compile };
