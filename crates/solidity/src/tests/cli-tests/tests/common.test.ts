import { executeCommand, isFolderExist, isFileExist, isFileEmpty, executeCommandWithStdin } from "../src/helper";
import { paths } from '../src/entities';
import * as shell from 'shelljs';
import * as path from 'path';



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

describe("Run resolc with source debug information", () => {
    const commands = [
        `resolc -g ${paths.pathToBasicSolContract}  --bin --asm --output-dir "${paths.pathToOutputDir}"`,
        `resolc --disable-solc-optimizer -g ${paths.pathToBasicSolContract}  --bin --asm --output-dir "${paths.pathToOutputDir}"`
    ]; // potential issue on resolc with full path on Windows cmd`;

    for (var idx in commands) {
        const command = commands[idx];
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
        it("Output files are created", () => { // a bug on windows
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
    }
});

describe("Run resolc with source debug information, check LLVM debug-info", () => {
    const commands = [
        `resolc -g ${paths.pathToBasicSolContract} --debug-output-dir="${paths.pathToOutputDir}"`,
        `resolc -g --disable-solc-optimizer ${paths.pathToBasicSolContract} --debug-output-dir="${paths.pathToOutputDir}"`
    ]; // potential issue on resolc with full path on Windows cmd`;

    for (var idx in commands) {
        const command = commands[idx];
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
        it("Output files are created", () => { // a bug on windows
            expect(isFileExist(paths.pathToOutputDir, paths.contractOptimizedLLVMFilename, paths.llvmExtension)).toBe(true);
            expect(isFileExist(paths.pathToOutputDir, paths.contractUnoptimizedLLVMFilename, paths.llvmExtension)).toBe(true);
        });
        it("the output files are not empty", () => {
            expect(isFileEmpty(paths.pathToSolBinOutputFile)).toBe(false);
            expect(isFileEmpty(paths.pathToSolAsmOutputFile)).toBe(false);
        });
        it("No 'Error'/'Fail' in the output", () => {
            expect(result.output).not.toMatch(/([Ee]rror|[Ff]ail)/i);
        });
    }
});



describe("Standard JSON compilation with path options", () => {
    const contractsDir = path.join(shell.tempdir(), 'contracts-test');
    const inputFile = path.join(__dirname, '..', 'src/contracts/compiled/1.json');

    beforeAll(() => {
        shell.mkdir('-p', contractsDir);

        const input = JSON.parse(shell.cat(inputFile).toString());

        Object.entries(input.sources).forEach(([sourcePath, source]: [string, any]) => {
            const filePath = path.join(contractsDir, sourcePath);
            shell.mkdir('-p', path.dirname(filePath));
            shell.ShellString(source.content).to(filePath);
        });
    });

    afterAll(() => {
        shell.rm('-rf', contractsDir);
    });

    describe("Output with all path options", () => {
        let result: { exitCode: number; output: string };

        beforeAll(async () => {
            const tempInputFile = path.join(contractsDir, 'temp-input.json');
            shell.cp(inputFile, tempInputFile);
            const inputContent = shell.cat(inputFile).toString();

            const command = `resolc --standard-json --base-path "${contractsDir}" --include-path "${contractsDir}" --allow-paths "${contractsDir}"`;

            result = await executeCommandWithStdin(command, inputContent);

            shell.rm(tempInputFile);

        });

        it("Compiler run successful without emiting warnings", () => {
            const parsedResults = JSON.parse(result.output)
            expect(parsedResults.errors.filter((error: { type: string; }) => error.type != 'Warning')).toEqual([]);
        });
    });
});