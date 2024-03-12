import {executeCommand} from "../src/helper";
import { paths } from '../src/entities';


//id1746
describe("Run with --asm by default", () => {
  const command = `zksolc ${paths.pathToBasicSolContract} --asm`;
  const result = executeCommand(command);
  const commandInvalid = 'zksolc --asm';
  const resultInvalid = executeCommand(commandInvalid);

  it("Valid command exit code = 0", () => {
    expect(result.exitCode).toBe(0);
  });

  it("--asm output is presented", () => {
      expect(result.output).toMatch(/(__entry:)/i);
  });


  it("solc exit code == zksolc exit code", () => {
      const command = `solc ${paths.pathToBasicSolContract} --asm`;
      const solcResult = executeCommand(command);
      expect(solcResult.exitCode).toBe(result.exitCode);
  });

  it("run invalid: zksolc --asm", () => {
    expect(resultInvalid.output).toMatch(/(No input sources specified|Compilation aborted)/i);
  });
  it("Invalid command exit code = 1", () => {
    expect(resultInvalid.exitCode).toBe(1);
  });

  it("Invalid solc exit code == Invalid zksolc exit code", () => {
    const command = 'solc --asm';
    const solcResult = executeCommand(command);
    expect(solcResult.exitCode).toBe(resultInvalid.exitCode);
  });
});
