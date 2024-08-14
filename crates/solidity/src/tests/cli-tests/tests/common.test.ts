import { executeCommand, isFolderExist, isFileExist, isFileEmpty } from "../src/helper";
import { paths } from '../src/entities';


//id1762
describe("Run resolc without any options", () => {
    const command = 'resolc';
    const result = executeCommand(command);

    it("Info with help is presented", () => {
        expect(result.output).toMatch(/(No input sources specified|Error(s) found.)/i);
    });

    it("Exit code = 1", () => {
        expect(result.exitCode).toBe(1);
    });

    it("solc exit code == resolc exit code", () => {
        const command = 'solc';
        const solcResult = executeCommand(command);
        expect(solcResult.exitCode).toBe(result.exitCode);
    });
});


//#1713
describe("Default run a command from the help", () => {

    const command = `resolc ${paths.pathToBasicSolContract} -O3 --bin --output-dir "${paths.pathToOutputDir}"`; // potential issue on resolc with full path on Windows cmd
    const result = executeCommand(command);

    it("Compiler run successful", () => {
        expect(result.output).toMatch(/(Compiler run successful.)/i);
    });
    it("Exit code = 0", () => {
        expect(result.exitCode).toBe(0);
    });
    it("Output dir is created", () => {
        expect(isFolderExist(paths.pathToOutputDir)).toBe(true);
    });
    xit("Output file is created", () => { // a bug on windows
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.binExtension)).toBe(true);
    });
    it("the output file is not empty", () => {
        expect(isFileEmpty(paths.pathToSolBinOutputFile)).toBe(false);
    });
    it("No 'Error'/'Warning'/'Fail' in the output", () => {
        expect(result.output).not.toMatch(/([Ee]rror|[Ww]arning|[Ff]ail)/i);
    });
});

//#1818
describe("Default run a command from the help", () => {

    const command = `resolc ${paths.pathToBasicSolContract} -O3 --bin --asm --output-dir "${paths.pathToOutputDir}"`; // potential issue on resolc with full path on Windows cmd
    const result = executeCommand(command);

    it("Compiler run successful", () => {
        expect(result.output).toMatch(/(Compiler run successful.)/i);
    });
    it("Exit code = 0", () => {
        expect(result.exitCode).toBe(0);
    });
    it("Output dir is created", () => {
        expect(isFolderExist(paths.pathToOutputDir)).toBe(true);
    });
    xit("Output files are created", () => { // a bug on windows
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.binExtension)).toBe(true);
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.asmExtension)).toBe(true);
    });
    it("the output files are not empty", () => {
        expect(isFileEmpty(paths.pathToSolBinOutputFile)).toBe(false);
        expect(isFileEmpty(paths.pathToSolAsmOutputFile)).toBe(false);
    });
    it("No 'Error'/'Warning'/'Fail' in the output", () => {
        expect(result.output).not.toMatch(/([Ee]rror|[Ww]arning|[Ff]ail)/i);
    });
});

describe("Run resolc with source debug information enabled", () => {

    const command = `resolc -g ${paths.pathToBasicSolContract}  --bin --asm --output-dir "${paths.pathToOutputDir}"`; // potential issue on resolc with full path on Windows cmd`;
    const result = executeCommand(command);

    it("Compiler run successful", () => {
        expect(result.output).toMatch(/(Compiler run successful.)/i);
    });
    it("Exit code = 0", () => {
        expect(result.exitCode).toBe(0);
    });
    it("Output dir is created", () => {
        expect(isFolderExist(paths.pathToOutputDir)).toBe(true);
    });
    xit("Output files are created", () => { // a bug on windows
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.binExtension)).toBe(true);
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.asmExtension)).toBe(true);
    });
    it("the output files are not empty", () => {
        expect(isFileEmpty(paths.pathToSolBinOutputFile)).toBe(false);
        expect(isFileEmpty(paths.pathToSolAsmOutputFile)).toBe(false);
    });
    it("No 'Error'/'Fail' in the output", () => {
        expect(result.output).not.toMatch(/([Ee]rror|[Ff]ail)/i);
    });
});

describe("Run resolc with source debug information and no solc optimization", () => {

    const command = `resolc --disable-solc-optimizer -g ${paths.pathToBasicSolContract}  --bin --asm --output-dir "${paths.pathToOutputDir}"`; // potential issue on resolc with full path on Windows cmd`;
    const result = executeCommand(command);

    it("Compiler run successful", () => {
        expect(result.output).toMatch(/(Compiler run successful.)/i);
    });
    it("Exit code = 0", () => {
        expect(result.exitCode).toBe(0);
    });
    it("Output dir is created", () => {
        expect(isFolderExist(paths.pathToOutputDir)).toBe(true);
    });
    xit("Output files are created", () => { // a bug on windows
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.binExtension)).toBe(true);
        expect(isFileExist(paths.pathToOutputDir, paths.contractSolFilename, paths.asmExtension)).toBe(true);
    });
    it("the output files are not empty", () => {
        expect(isFileEmpty(paths.pathToSolBinOutputFile)).toBe(false);
        expect(isFileEmpty(paths.pathToSolAsmOutputFile)).toBe(false);
    });
    it("No 'Error'/'Fail' in the output", () => {
        expect(result.output).not.toMatch(/([Ee]rror|[Ff]ail)/i);
    });
});
