import {executeCommand} from "../src/helper";
import { paths } from '../src/entities';


//id1746
describe("Run with --asm by default", () => {
  const command = `resolc ${paths.pathToBasicSolContract} --asm`;
  const result = executeCommand(command);
  const commandInvalid = 'resolc --asm';
  const resultInvalid = executeCommand(commandInvalid);

  it("Valid command exit code = 0", () => {
    expect(result.exitCode).toBe(0);
  });

  it("--asm output is presented", () => {
    const expectedPatterns = [/(deploy)/i, /(call)/i, /(seal_return)/i];

    for (const pattern of expectedPatterns) {
      expect(result.output).toMatch(pattern);
    }
  });

  it("solc exit code == resolc exit code", () => {
    const command = `solc ${paths.pathToBasicSolContract} --asm`;
    const solcResult = executeCommand(command);
    expect(solcResult.exitCode).toBe(result.exitCode);
  });

  it("run invalid: resolc --asm", () => {
    expect(resultInvalid.output).toMatch(/(No input sources specified|Compilation aborted)/i);
  });
  
  it("Invalid command exit code = 1", () => {
    expect(resultInvalid.exitCode).toBe(1);
  });

  it("Invalid solc exit code == Invalid resolc exit code", () => {
    const command = 'solc --asm';
    const solcResult = executeCommand(command);
    expect(solcResult.exitCode).toBe(resultInvalid.exitCode);
  });
});