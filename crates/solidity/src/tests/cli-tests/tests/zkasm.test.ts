import {executeCommand} from "../src/helper";
import { paths } from '../src/entities';


//id1745
describe("Run with --zkasm by default", () => {
  const command = `zksolc ${paths.pathToBasicZkasmContract} --zkasm`;
  const result = executeCommand(command);

  it("Valid command exit code = 0", () => {
    expect(result.exitCode).toBe(0);
  });

  it("--zkasm output is presented", () => {
      expect(result.output).toMatch(/(Compiler run successful)/i);
      expect(result.output).toMatch(/(No output requested)/i);
  });
});
