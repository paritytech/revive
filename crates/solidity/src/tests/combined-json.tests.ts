import { executeCommand } from "../src/helper";
import { paths } from '../src/entities';


describe("Set of --combined-json tests", () => {
    const zksolcCommand = 'zksolc';
    const solcCommand = 'solc';
    const json_args: string[] = [`abi`, `hashes`, `metadata`, `devdoc`, `userdoc`, `storage-layout`, `ast`, `asm`, `bin`, `bin-runtime`];

    //id1742:I
    describe(`Run ${zksolcCommand} with just --combined-json`, () => {
        const args = [`--combined-json`];
        const result = executeCommand(zksolcCommand, args);

        it("Valid command exit code = 1", () => {
            expect(result.exitCode).toBe(1);
        });

        it("--combined-json error is presented", () => {
            expect(result.output).toMatch(/(requires a value but none was supplied)/i);
        });

        it("solc exit code == zksolc exit code", () => {
            const solcResult = executeCommand(solcCommand, args);
            expect(solcResult.exitCode).toBe(result.exitCode);
        });
    });

    //id1742:II
    describe(`Run ${zksolcCommand} with Sol contract and --combined-json`, () => {
        const args = [`${paths.pathToBasicSolContract}`, `--combined-json`];
        const result = executeCommand(zksolcCommand, args);

        it("Valid command exit code = 1", () => {
            expect(result.exitCode).toBe(1);
        });

        it("--combined-json error is presented", () => {
            expect(result.output).toMatch(/(requires a value but none was supplied)/i);
        });

        it("solc exit code == zksolc exit code", () => {
            const solcResult = executeCommand(solcCommand, args);
            expect(solcResult.exitCode).toBe(result.exitCode);
        });
    });

    //id1742:III
    for (let i = 0; i < json_args.length; i++) {
        describe(`Run ${zksolcCommand} with Sol, --combined-json and ARG: ${json_args[i]}`, () => {
            const args = [`${paths.pathToBasicSolContract}`, `--combined-json`, `${json_args[i]}`];
            const result = executeCommand(zksolcCommand, args);

            it("Valid command exit code = 0", () => {
                expect(result.exitCode).toBe(0);
            });

            it("--combined-json error is presented", () => {
                expect(result.output).toMatch(/(contracts)/i);
            });

            it("solc exit code == zksolc exit code", () => {
                const solcResult = executeCommand(solcCommand, args);
                expect(solcResult.exitCode).toBe(result.exitCode);
            });
        });
    }

    //id1829:I
    for (let i = 0; i < json_args.length; i++) {
        describe(`Run ${zksolcCommand} with Sol, --combined-json and wrong ARG: --${json_args[i]}`, () => {
            const args = [`${paths.pathToBasicSolContract}`, `--combined-json`, `--${json_args[i]}`];
            const result = executeCommand(zksolcCommand, args);

            it("Valid command exit code = 1", () => {
                expect(result.exitCode).toBe(1);
            });

            it("--combined-json error is presented", () => {
                expect(result.output).toMatch(/(Invalid option|error)/i);
            });

            it("solc exit code == zksolc exit code", () => {
                const solcResult = executeCommand(solcCommand, args);
                expect(solcResult.exitCode).toBe(result.exitCode);
            });
        });
    }

    //id1829:II
    for (let i = 0; i < json_args.length; i++) {
        describe(`Run ${zksolcCommand} with Sol, --combined-json and multiple ARG: ${json_args[i]} ${json_args[i]}`, () => {
            const args = [`${paths.pathToBasicSolContract}`, `--combined-json`, `${json_args[i]}`, `${json_args[i]}`];
            const result = executeCommand(zksolcCommand, args);

            xit("Valid command exit code = 1", () => {
                expect(result.exitCode).toBe(1);
            });

            it("--combined-json error is presented", () => {
                expect(result.output).toMatch(/(No such file or directory|cannot find the file specified)/i); // Hopefully we should have more precise message here!
            });

            xit("solc exit code == zksolc exit code", () => {
                const solcResult = executeCommand(solcCommand, args);
                expect(solcResult.exitCode).toBe(result.exitCode);
            });
        });
    }

    //id1829:III
    for (let i = 0; i < json_args.length; i++) {
        describe(`Run ${zksolcCommand} with Sol, and multiple (--combined-json ${json_args[i]})`, () => {
            const args = [`${paths.pathToBasicSolContract}`, `--combined-json`, `${json_args[i]}`, `--combined-json`, `${json_args[i]}`];
            const result = executeCommand(zksolcCommand, args);

            it("Valid command exit code = 1", () => {
                expect(result.exitCode).toBe(1);
            });

            it("--combined-json error is presented", () => {
                expect(result.output).toMatch(/(cannot be used multiple times)/i);
            });

            it("solc exit code == zksolc exit code", () => {
                const solcResult = executeCommand(solcCommand, args);
                expect(solcResult.exitCode).toBe(result.exitCode);
            });
        });
    }

    //id1830
    for (let i = 0; i < json_args.length; i++) {
        describe(`Run ${zksolcCommand} with Yul, and --combined-json ${json_args[i]}`, () => {
            const args = [`${paths.pathToBasicYulContract}`, `--combined-json`, `${json_args[i]}`];
            const result = executeCommand(zksolcCommand, args);

            it("Valid command exit code = 1", () => {
                expect(result.exitCode).toBe(1);
            });

            it("--combined-json error is presented", () => {
                expect(result.output).toMatch(/(ParserError: Expected identifier)/i);
            });
            asd

            it("solc exit code == zksolc exit code", () => {
                const solcResult = executeCommand(solcCommand, args);
                expect(solcResult.exitCode).toBe(result.exitCode);
            });
        });
    }
});
