import {executeCommand} from "../src/helper";
import { paths } from '../src/entities';


//id1743
describe("Run with --yul by default", () => {
  const command = `resolc ${paths.pathToBasicYulContract} --yul`;
  const result = executeCommand(command);
  const commandInvalid = 'resolc --yul';
  const resultInvalid = executeCommand(commandInvalid);

  it("Valid command exit code = 0", () => {
    expect(result.exitCode).toBe(0);
  });

  it("--yul output is presented", () => {
    expect(result.output).toMatch(/(Compiler run successful)/i);
    expect(result.output).toMatch(/(No output requested)/i);
  });


  xit("solc exit code == resolc exit code", () => { // unknown solc issue for datatype of the contract
      const command = `solc ${paths.pathToBasicSolContract} --yul`;
      const solcResult = executeCommand(command);
      expect(solcResult.exitCode).toBe(result.exitCode);
  });

  it("run invalid: resolc --yul", () => {
    expect(resultInvalid.output).toMatch(/(The input file is missing)/i);
  });
  it("Invalid command exit code = 1", () => {
    expect(resultInvalid.exitCode).toBe(1);
  });

  it("Invalid solc exit code == Invalid resolc exit code", () => {
    const command = 'solc --yul';
    const solcResult = executeCommand(command);
    expect(solcResult.exitCode).toBe(resultInvalid.exitCode);
  });
});
