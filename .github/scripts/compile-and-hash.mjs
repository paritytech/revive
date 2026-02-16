#!/usr/bin/env node

// @ts-check

/**
 * This script compiles all `.sol` and `.yul` files from the provided projects
 * directories (and its subdirectories) using the provided resolc (either native
 * binary or Wasm) and generates SHA256 hashes of each contract's bytecode.
 *
 * This script handles both native binaries and Wasm builds with shared logic.
 * Wasm mode is enabled when soljson is provided.
 *
 * For Solidity files:
 * - Each top-level item within each directory provided is treated as a single compilation unit/project:
 *   - Top-level files are compiled individually
 *   - Top-level subdirectories are compiled as one unit (their files are compiled together)
 *
 * This ensures that imports between files in the same subdirectory resolve automatically
 * on both native and Wasm platforms, while allowing parallel compilations across units,
 * failures to be isolated so other units still continue to compile, and a bounded output size.
 *
 * For Yul files:
 * - All Yul files are treated as single-file compilation units as only one input file is supported.
 *
 * ```
 * Usage: node compile-and-hash.mjs [options]
 *
 * Options:
 *   --resolc:         Path to native binary or Node.js module for Wasm
 *   --base-dir:       Common base directory for all `projects-dirs`
 *   --projects-dirs:  Comma-separated list of directories containing .sol (Solidity) and/or .yul (Yul) files
 *                     - Each top-level file or subdirectory within a projects directory is compiled as a single unit/project
 *                     - Example:
 *                       - Input: "contracts/solidity/simple, contracts/solidity/complex, contracts/yul"
 *                       - Meaning: All top-level files and subdirectories within "contracts/solidity/simple",
 *                                  "contracts/solidity/complex", and "contracts/yul" are compiled together as
 *                                  units/projects if Solidity. Yul files are always compiled individually.
 *   --output-file:    File path to write the output JSON to (the file and parent directories are created automatically)
 *   --opt-levels:     Comma-separated optimization levels (e.g., "0,3,z")
 *   --platform-label: Label for the platform (e.g., linux, macos, windows, wasm)
 *   [--soljson]:      Path to soljson for Wasm builds (omit for native)
 *   [--debug]:        Enable verbose debug output
 *   [--help]:         Show the usage
 * ```
 *
 * Examlple output format:
 * - Hashes are grouped by optimization level
 * - The file paths used as keys are normalized for cross-platform comparison.
 *   They should thereby not be used to access the filesystem.
 *
 * ```json
 * {
 *   "platform": "linux",
 *   "hashes": {
 *     "0": {
 *       "solidity/simple/loop/array/simple.sol": {
 *         "ContractNameA": "<hash>",
 *         "ContractNameB": "<hash>"
 *       },
 *       "yul/instructions/byte.yul": {
 *         "ContractNameA": "<hash>"
 *       }
 *     },
 *     "3": { ... },
 *     "z": { ... }
 *   },
 *   "failedPaths": {
 *     "0": {
 *       "path/to/failed/unit": [
 *         "Error message1...",
 *         "Error message2..."
 *       ],
 *     },
 *     "3": { ... },
 *     "z": { ... }
 *   }
 * }
 * ```
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import util from "node:util";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { execFile } from "node:child_process";

const execFileAsync = util.promisify(execFile);

/**
 * Allows loading CommonJS modules (e.g. resolc.js, soljson.js) from ES modules.
 */
const require = createRequire(import.meta.url);

const VALID_OPT_LEVELS = ["0", "1", "2", "3", "s", "z"];
const PVM_BYTECODE_PREFIX = "50564d";
const DEFAULT_CONCURRENCY = Math.min(5, os.availableParallelism?.() ?? os.cpus().length);

const Language = Object.freeze({
    Solidity: "Solidity",
    Yul: "Yul",
});

/**
 * @typedef {typeof Language[keyof typeof Language]} LanguageKind
 */

/**
 * @typedef {Object} SourceFile
 * @property {string} platformSpecificPath - The platform-specific file path.
 * @property {string} normalizedPathId - The normalized file path for cross-platform consistency.
 * @property {string} content - The file content.
 */

/**
 * @typedef {Object} CompilationUnit
 * @property {string} platformSpecificRootPath - The top-level platform-specific file or directory path of the unit.
 * @property {string} normalizedRootPathId - The top-level normalized file or directory path of the unit.
 * @property {SourceFile[]} files - The source files in this unit.
 */

/**
 * @typedef {Object} Contract
 * @property {string} normalizedPathId - The normalized file path of the contract.
 * @property {string} name - The name of the contract.
 * @property {string} bytecode - The compiled bytecode as a hex string.
 */

/**
 * @typedef {Object} CompilationResult
 * @property {Contract[]} contracts - The compiled contracts.
 * @property {string[]} errors - Compilation errors if any.
 */

/**
 * @typedef {{[contractName: string]: string}} HashEntry
 */

/**
 * @typedef {Object} HashResult
 * @property {{[optLevel: string]: {[path: string]: HashEntry}}} hashes - The hashes for each normalized path keyed by optimization level.
 * @property {{[optLevel: string]: {[path: string]: string[]}}} failedPaths - The normalized root paths of the units that failed to compile and their errors, keyed by optimization level.
 */

/**
 * @typedef {Object} ResolcWasm
 * @property {unknown} soljson - The soljson module.
 * @property {(data: string) => void} writeToStdin - Writes data to stdin.
 * @property {(args: string[]) => number} callMain - Invokes the compiler with arguments.
 * @property {() => string} readFromStdout - Reads the stdout content.
 * @property {() => string} readFromStderr - Reads the stderr content.
 */

/**
 * @typedef {Object} CompileConfig
 * @property {string} resolcPath - The path to the resolc build.
 * @property {string[]} optLevels - The optimization levels to compile with.
 * @property {(() => ResolcWasm) | null} createResolc - The Wasm resolc factory (null for native).
 * @property {unknown} soljson - The soljson module (null for native).
 * @property {boolean} debug - Whether to enable verbose output.
 */

/**
 * Custom error for invalid usage.
 */
class ValidationError extends Error {
    /** @type {boolean} */
    showUsage;

    /**
     * @param {string} message - The error message.
     * @param {{ showUsage?: boolean }} [options] - Whether to show usage information.
     */
    constructor(message, { showUsage = false } = {}) {
        super(`Error: ${message}`);
        this.name = "ValidationError";
        this.showUsage = showUsage;
    }
}

/**
 * Argument specifications defining CLI options.
 */
const ARGUMENT_SPECS = {
    resolcPath: {
        cliName: "resolc",
        description: "Path to native binary or Node.js module for Wasm",
        type: "string",
        required: true,
        /** @param {string} value */
        parse: (value) => path.resolve(value),
    },
    baseDir: {
        cliName: "base-dir",
        description: "Common base directory for all projects",
        type: "string",
        required: true,
        /** @param {string} value */
        parse: (value) => path.resolve(value),
    },
    projectsDirs: {
        cliName: "projects-dirs",
        description: [
            "Comma-separated list of directories containing .sol (Solidity) and/or .yul (Yul) files",
            "- Each top-level file or subdirectory within a projects directory is compiled as a single unit/project",
            "- Example:",
            '  - Input: "contracts/solidity/simple, contracts/solidity/complex, contracts/yul"',
            '  - Meaning: All top-level files and subdirectories within "contracts/solidity/simple",',
            '             "contracts/solidity/complex", and "contracts/yul" are compiled together as',
            "             units/projects if Solidity. Yul files are always compiled individually."
        ].join("\n".padEnd(24)),
        type: "string",
        required: true,
        /** @param {string} value */
        parse: (value) => [...new Set(value.split(",").map(dir => path.resolve(dir.trim())))],
    },
    outputFile: {
        cliName: "output-file",
        description: "File path to write the output JSON to (parent directories are created automatically)",
        type: "string",
        required: true,
        /** @param {string} value */
        parse: (value) => path.resolve(value),
    },
    optLevels: {
        cliName: "opt-levels",
        description: 'Comma-separated optimization levels (e.g., "0,3,z")',
        type: "string",
        required: true,
        /** @param {string} value */
        parse: (value) => [...new Set(value.split(",").map((opt) => opt.trim().toLowerCase()))],
    },
    platformLabel: {
        cliName: "platform-label",
        description: "Label for the platform (e.g., linux, macos, windows, wasm)",
        type: "string",
        required: true,
        /** @param {string} value */
        parse: (value) => value,
    },
    soljsonPath: {
        cliName: "soljson",
        description: "Path to soljson for Wasm builds (omit for native)",
        type: "string",
        required: false,
        /** @param {string | undefined} value */
        parse: (value) => value ? path.resolve(value) : null,
    },
    debug: {
        cliName: "debug",
        description: "Enable verbose debug output",
        type: "boolean",
        required: false,
        /** @param {boolean | undefined} value */
        parse: (value) => !!value,
    },
    help: {
        cliName: "help",
        description: "Show the usage",
        type: "boolean",
        required: false,
        /** @param {boolean | undefined} value */
        parse: (value) => !!value,
    },
};

/**
 * Parsed arguments derived from {@link ARGUMENT_SPECS}.
 * Each key maps to the return type of its corresponding parse function.
 * @typedef {{ [K in keyof typeof ARGUMENT_SPECS]: ReturnType<(typeof ARGUMENT_SPECS)[K]["parse"]> }} ParsedArguments
 */

/**
 * Parses and validates command-line arguments.
 * @returns {ParsedArguments} The parsed arguments.
 * @throws {ValidationError} If arguments are invalid.
 */
function parseArguments() {
    const argumentSpecs = Object.values(ARGUMENT_SPECS);

    /** @type {util.ParseArgsOptionsConfig} */
    const optionsConfig = {};
    for (const spec of argumentSpecs) {
        optionsConfig[spec.cliName] = /** @type {util.ParseArgsOptionDescriptor} */ ({ type: spec.type });
    }

    /** @type {ReturnType<typeof util.parseArgs>["values"]} */
    let values;
    try {
        values = util.parseArgs({ options: optionsConfig, strict: true }).values;
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new ValidationError(message, { showUsage: true });
    }

    const missing = argumentSpecs
        .filter((spec) => spec.required && !values[spec.cliName])
        .map((spec) => `--${spec.cliName}`);
    if (missing.length > 0) {
        throw new ValidationError(`Missing required arguments: ${missing.join(", ")}`, { showUsage: true });
    }

    return /** @type {ParsedArguments} */ (Object.fromEntries(
        Object.entries(ARGUMENT_SPECS).map(([name, spec]) => [
            name,
            // @ts-expect-error - Types have been validated (util.parseArgs's `values` only define unions).
            spec.parse(values[spec.cliName]),
        ])
    ));
}

/**
* @returns {string} The usage for the script.
 */
function getUsage() {
    const lines = [
        "Usage: node compile-and-hash.mjs [options]",
        "",
        "Options:"
    ];

    for (const spec of Object.values(ARGUMENT_SPECS)) {
        const flag = spec.required ? `--${spec.cliName}` : `[--${spec.cliName}]`;
        lines.push(`  ${flag.padEnd(20)} ${spec.description}`);
    }

    return lines.join("\n");
}

/**
 * Computes a SHA256 hash of a string.
 * @param {string} data - The input string to hash.
 * @returns {string} The hex-encoded SHA256 hash.
 */
function sha256(data) {
    return createHash("sha256").update(data).digest("hex");
}

/**
 * Reads a file and returns its path and content.
 * @param {string} baseDir - The common base directory for all units being compiled (used for normalization).
 * @param {string} filePath - The file path to read.
 * @returns {Promise<SourceFile>} The file with its content.
 */
async function readFile(baseDir, filePath) {
    return {
        platformSpecificPath: filePath,
        normalizedPathId: getNormalizedPathId(baseDir, filePath),
        content: await fs.promises.readFile(filePath, "utf-8"),
    };
}

/**
 * Recursively finds all files with the given extension in a directory.
 * Requires Node.js 20.12.0+ for recursive `readdir` with `parentPath` support.
 * @param {string} directory - The directory to search in.
 * @param {string} extension - The file extension to match (e.g., ".sol").
 * @returns {Promise<string[]>} File paths in platform-specific format.
 */
async function findFiles(directory, extension) {
    const entries = await fs.promises.readdir(directory, { recursive: true, withFileTypes: true });
    return entries
        .filter(entry => entry.isFile() && path.extname(entry.name) === extension)
        .map(entry => path.join(entry.parentPath, entry.name));
}

/**
 * Discovers and loads all files with `extension` in `directory` and all its
 * subdirectories as single-file compilation units.
 * @param {string} baseDir - The common base directory for all units being compiled (used for normalization).
 * @param {string} startDir - The directory to start the search.
 * @param {string} extension - The file extension to match.
 * @returns {Promise<CompilationUnit[]>} Compilation units each containing one file.
 */
async function loadSingleFileCompilationUnits(baseDir, startDir, extension) {
    const filePaths = await findFiles(startDir, extension);
    const files = await Promise.all(filePaths.map((filePath) => readFile(baseDir, filePath)));

    return files.map((file) => ({
        platformSpecificRootPath: file.platformSpecificPath,
        normalizedRootPathId: file.normalizedPathId,
        files: [file],
    }));
}

/**
 * Discovers and loads all files with `extension` in `directory` and all its
 * subdirectories. Generates compilation units/projects with one or more files
 * from each top-level item in `directory`, ensuring that imports between files
 * in the same subdirectory resolve automatically on both native and Wasm platforms,
 * while allowing parallel compilations across units, failures to be isolated
 * so other units still continue to compile, and a bounded output size.
 * - Top-level files become individual compilation units (single file)
 * - Top-level subdirectories become multi-file compilation units (all files within)
 * @param {string} baseDir - The common base directory for all units being compiled (used for normalization).
 * @param {string} startDir - The directory to search.
 * @param {string} extension - The file extension to match.
 * @returns {Promise<CompilationUnit[]>} Compilation units each containing its respective files.
 */
async function loadTopLevelCompilationUnits(baseDir, startDir, extension) {
    /** @type {CompilationUnit[]} */
    const units = [];
    const entries = await fs.promises.readdir(startDir, { withFileTypes: true });

    for (const entry of entries) {
        const unitRootPath = path.join(startDir, entry.name);
        const normalizedUnitRootPathId = getNormalizedPathId(baseDir, unitRootPath);

        // Top-level file: single-file unit.
        if (entry.isFile() && path.extname(entry.name) === extension) {
            units.push({
                platformSpecificRootPath: unitRootPath,
                normalizedRootPathId: normalizedUnitRootPathId,
                files: [await readFile(baseDir, unitRootPath)],
            });
        }
        // Top-level directory: multi-file unit.
        else if (entry.isDirectory()) {
            const filePaths = await findFiles(unitRootPath, extension);
            if (filePaths.length > 0) {
                units.push({
                    platformSpecificRootPath: unitRootPath,
                    normalizedRootPathId: normalizedUnitRootPathId,
                    files: await Promise.all(filePaths.map((filePath) => readFile(baseDir, filePath))),
                });
            }
        }
    }

    return units;
}

/**
 * Loads all compilation units/projects from multiple base directories.
 * @param {string} baseDir - The common base directory for all units being compiled (used for normalization).
 * @param {string[]} startDirs - The directories to start the searches from.
 * @param {LanguageKind} language - The source code language.
 * @returns {Promise<CompilationUnit[]>} All compilation units each containing its respective files.
 */
async function loadCompilationUnits(baseDir, startDirs, language) {
    /** @type {CompilationUnit[][]} */
    let units = [];
    switch (language) {
        case Language.Solidity: {
            units = await Promise.all(startDirs.map(startDir => loadTopLevelCompilationUnits(baseDir, startDir, ".sol")));
            break;
        }
        case Language.Yul: {
            // Only one input file is supported in Yul mode.
            units = await Promise.all(startDirs.map(startDir => loadSingleFileCompilationUnits(baseDir, startDir, ".yul")));
            break;
        }
        default:
            throw new Error(`Unsupported language: ${language}`);
    }

    return units.flat();
}

/**
 * Converts `filePath` to a relative path from the base directory and normalizes it with forward slashes.
 * This ensures that the file path ID added as part of a hash entry is consistent, and thus
 * comparable, across platforms. It should thereby not be used to access the filesystem.
 * @param {string} baseDir - The base directory common for all units to derive the relative path from.
 * @param {string} filePath - The file path.
 * @returns {string} The relative path with forward slashes.
 *
 * @example
 * ```
 * baseDir          = "/home/runner/work/contracts"
 * filePath         = "/home/runner/work/contracts/solidity/simple/loop.sol"
 * normalizedPathId = "solidity/simple/loop.sol"
 * ```
 *
 * @example
 * ```
 * baseDir          = "C:\\Users\\runner\\work\\contracts"
 * filePath         = "C:\\Users\\runner\\work\\contracts\\solidity\\simple\\loop.sol"
 * normalizedPathId = "solidity/simple/loop.sol"
 * ```
 */
function getNormalizedPathId(baseDir, filePath) {
    return path.relative(baseDir, filePath).replace(/\\/g, "/");
}

/**
 * Writes the result as a JSON file.
 * @param {string} outputPath - The path to write the JSON file.
 * @param {string} platform - The platform label.
 * @param {HashResult} result - The hash result to write.
 */
function writeResult(outputPath, platform, result) {
    const output = {
        platform,
        hashes: result.hashes,
        failedPaths: result.failedPaths,
    };
    fs.writeFileSync(outputPath, JSON.stringify(output, null, 2) + "\n");
}

/**
 * Counts the total number of hashes found in `hashes`.
 * @param {{[path: string]: HashEntry}} hashes - The hashes for each path to count.
 * @returns {number} The total number of hashes.
 */
function countHashes(hashes) {
    return Object.values(hashes).reduce(
        (sum, hashesAtPath) => sum + Object.keys(hashesAtPath).length,
        0
    );
}

/**
 * Creates standard JSON input for resolc.
 * @param {SourceFile[]} files - The source files.
 * @param {LanguageKind} language - The source code language.
 * @param {string} optLevel - The optimization level.
 * @returns {Object} The standard JSON input object.
 */
function createStandardJsonInput(files, language, optLevel) {
    /** @type {{[path: string]: {content: string}}} */
    const sources = {};
    for (const file of files) {
        // Use normalized paths as keys to make import resolution consistent across platforms.
        sources[file.normalizedPathId] = { content: file.content };
    }

    return {
        language,
        sources,
        settings: {
            optimizer: {
                enabled: true,
                mode: optLevel,
            },
            viaIR: true,
            outputSelection: {
                "*": {
                    "*": ["evm.bytecode"],
                },
            },
        },
    };
}

/**
 * Extracts the contract names and bytecodes from standard JSON output.
 * @param {string} output - The standard JSON compilation output.
 * @returns {CompilationResult} The extracted contracts and any potential errors.
 */
function parseStandardJsonOutput(output) {
    /**
    * @typedef {Object} StandardJsonOutput
    * @property {Array<{ severity: "error" | "warning" | "info", message: string, formattedMessage?: string }>} [errors] - Errors, warnings, and info.
    * @property {Record<string, Record<string, { evm?: { bytecode?: { object?: string } } }>>} [contracts] - Contract-level output.
    */
    /** @type {StandardJsonOutput} */
    const parsed = JSON.parse(output);

    /** @type {Contract[]} */
    const contracts = [];
    for (const [normalizedPathId, fileContracts] of Object.entries(parsed.contracts || {})) {
        for (const [name, contract] of Object.entries(fileContracts)) {
            const bytecode = contract.evm?.bytecode?.object;
            if (bytecode?.startsWith(PVM_BYTECODE_PREFIX)) {
                contracts.push({ normalizedPathId, name, bytecode });
            }
        }
    }

    const errors = (parsed.errors || [])
        .filter((error) => error.severity === "error")
        .map((error) => error.formattedMessage || error.message);

    return { contracts, errors };
}

/**
 * Compiles source files using the native resolc binary.
 * @param {string} resolcBinary - The path to the resolc binary.
 * @param {SourceFile[]} files - The source files to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {string} optLevel - The optimization level (e.g., "0", "3", "z").
 * @returns {Promise<CompilationResult>} The compilation result.
 * @throws {Error} For system-level errors.
 */
async function compileNative(resolcBinary, files, language, optLevel) {
    const input = createStandardJsonInput(files, language, optLevel);

    // In standard JSON mode, compilation failures are reported as part
    // of the JSON output and will exit in a success state. System-level
    // errors will throw exceptions which are intentionally bubbled up.
    const promise = execFileAsync(resolcBinary, ["--standard-json"], {
        // 200-second timeout for large multi-file units.
        timeout: 200_000,
        // 200MB buffer for large outputs.
        maxBuffer: 200 * 1024 * 1024,
    });
    if (!promise.child?.stdin) {
        throw new Error("Failed to spawn child process for resolc");
    }
    promise.child.stdin.write(JSON.stringify(input));
    promise.child.stdin.end();
    const { stdout } = await promise;

    return parseStandardJsonOutput(stdout);
}

/**
 * Compiles source files using the Node module for Wasm.
 * @param {() => ResolcWasm} createResolc - The Wasm resolc factory.
 * @param {unknown} soljson - The soljson module.
 * @param {SourceFile[]} files - The source files to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {string} optLevel - The optimization level (e.g., "0", "3", "z").
 * @returns {CompilationResult} The compilation result.
 * @throws {Error} For system-level errors.
 */
function compileWasm(createResolc, soljson, files, language, optLevel) {
    const compiler = createResolc();
    compiler.soljson = soljson;

    const input = createStandardJsonInput(files, language, optLevel);
    compiler.writeToStdin(JSON.stringify(input));

    // In standard JSON mode, compilation failures are reported as part
    // of the JSON output and will exit in a success state. System-level
    // errors cause a non-zero exit code returned which should be thrown.
    const exitCode = compiler.callMain(["--standard-json"]);

    if (exitCode !== 0) {
        const stderr = compiler.readFromStderr();
        // TODO: Temporarily filtering out resolc ICE errors so that it behaves similar to how
        //       native resolc outputs the error into the stdout JSON rather than stderr.
        //       (See: https://github.com/paritytech/revive/issues/476)
        const isInternalCompilerError = stderr.includes("ICE:");
        if (isInternalCompilerError) {
            return { contracts: [], errors: [stderr] };
        }
        throw new Error(`Compilation exited with code ${exitCode}: ${stderr}`);
    }

    return parseStandardJsonOutput(compiler.readFromStdout());
}

/**
 * Compiles a compilation unit at all optimization levels provided and hashes the bytecode.
 * @param {CompilationUnit} unit - The compilation unit to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {CompileConfig} config - The compilation configuration.
 * @returns {Promise<HashResult>} Hashes and files that failed to compile for each optimization level.
 */
async function compileAndHashUnit(unit, language, config) {
    const { resolcPath, optLevels, createResolc, soljson, debug } = config;
    const isWasm = typeof createResolc === "function";

    if (debug) {
        console.log(`[DEBUG] Compiling unit with ${unit.files.length} file${unit.files.length === 1 ? "" : "s"}: ${unit.platformSpecificRootPath}`);
    }

    /** @type {HashResult} */
    const result = {
        hashes: {},
        failedPaths: {},
    };

    for (const optLevel of optLevels) {
        result.hashes[optLevel] = {};
        result.failedPaths[optLevel] = {};

        const { contracts, errors } = isWasm
            ? compileWasm(createResolc, soljson, unit.files, language, optLevel)
            : await compileNative(resolcPath, unit.files, language, optLevel);

        if (errors.length > 0) {
            if (debug) {
                console.warn(`[DEBUG] Errors compiling file(s) in unit \`${unit.platformSpecificRootPath}\` at optimization \`${optLevel}\`:\n- ${errors.join("\n- ")}`);
            }
            result.failedPaths[optLevel][unit.normalizedRootPathId] = errors;
        }

        for (const { normalizedPathId, name, bytecode } of contracts) {
            if (!result.hashes[optLevel][normalizedPathId]) {
                result.hashes[optLevel][normalizedPathId] = {};
            }
            result.hashes[optLevel][normalizedPathId][name] = sha256(bytecode);
        }
    }

    return result;
}

/**
 * Compiles all units and collects hashes.
 * @param {CompilationUnit[]} units - The compilation units to process.
 * @param {LanguageKind} language - The source code language.
 * @param {CompileConfig} config - The compilation configuration.
 * @returns {Promise<HashResult[]>} Per-unit hash results.
 */
async function compileAndHashAll(units, language, config) {
    const totalFiles = units.reduce((sum, unit) => sum + unit.files.length, 0);
    console.log();
    console.log(`=== Compiling ${totalFiles} ${language} files (${units.length} compilation units, batch size: ${DEFAULT_CONCURRENCY}) ===`);
    console.log();

    // Sort by path (byte-order, not `localCompare()`) for deterministic batch-level processing order, helpful for debugging.
    units.sort((a, b) => Number(a.normalizedRootPathId > b.normalizedRootPathId) - Number(a.normalizedRootPathId < b.normalizedRootPathId));

    /** @param {number} numUnitsProcessed */
    const reportProgress = (numUnitsProcessed) => console.log(`Processed ${numUnitsProcessed}/${units.length} units...`);

    /** @param {CompilationUnit} unit */
    const compile = (unit) => compileAndHashUnit(unit, language, config);

    return batch(compile, units, DEFAULT_CONCURRENCY, reportProgress);
}

/**
 * Processes items in parallel batches of a fixed size.
 * @note
 * This uses fixed batches rather than a pool to preserve batch-level determinism:
 * - Items are always processed in the same concurrent groups across runs
 * - `onBatchComplete` fires at predictable intervals
 * - Failures can be attributed to a specific batch
 * - Log output is comparable across runs and platforms
 * @template T, R
 * @param {(item: T) => Promise<R>} process - The function to process each item.
 * @param {T[]} items - The items to process.
 * @param {number} batchSize - The number of items to process in parallel per batch.
 * @param {((numItemsProcessed: number) => void)|undefined} onBatchComplete - A callback called after each completed batch with the cumulative count processed.
 * @returns {Promise<R[]>} The results from all processed items.
 */
async function batch(process, items, batchSize, onBatchComplete) {
    /** @type {R[]} */
    const result = [];
    for (let i = 0; i < items.length; i += batchSize) {
        const currentBatch = items.slice(i, i + batchSize);
        const batchResult = await Promise.all(currentBatch.map(process));
        result.push(...batchResult);

        if (typeof onBatchComplete === "function") {
            const numItemsProcessed = Math.min(i + batchSize, items.length);
            onBatchComplete(numItemsProcessed);
        }
    }

    return result;
}

/**
 * Aggregates multiple per-unit compilation and hash results into a single result.
 * @param {HashResult[]} results - The per-unit compilation and hash results to aggregate.
 * @param {string[]} optLevels - The optimization levels used during compilation.
 * @returns {{result: HashResult, totalHashes: number}} Combined hash results and the total number of hashes generated.
 */
function aggregateResults(results, optLevels) {
    /** @type {HashResult} */
    const aggregatedResult = {
        hashes: {},
        failedPaths: {},
    };
    let totalHashes = 0;

    for (const optLevel of optLevels) {
        aggregatedResult.hashes[optLevel] = {};
        aggregatedResult.failedPaths[optLevel] = {};

        for (const partialResult of results) {
            Object.assign(aggregatedResult.hashes[optLevel], partialResult.hashes[optLevel]);
            Object.assign(aggregatedResult.failedPaths[optLevel], partialResult.failedPaths[optLevel]);
        }

        totalHashes += countHashes(aggregatedResult.hashes[optLevel]);
    }

    return { result: aggregatedResult, totalHashes };
}

/**
 * Builds a final report from the results.
 * @param {HashResult} result - The aggregated compilation and hash result.
 * @param {number} totalUnits - The total number of units processed.
 * @param {number} totalHashes - The total number of hashes generated.
 * @param {string[]} optLevels - The optimization levels used during compilation.
 * @param {string} outputFile - The output file where the hash results can be found.
 * @returns {string} The report.
 *
 * @example
 * ```
 * ===========================================
 * SUMMARY
 * ===========================================
 *
 * Optimization level 0:
 * ---------------------
 *
 *     2588 hashes generated, 223/225 units compiled
 *     2 units failed to compile:
 *         - solidity/simple/immutable_evm
 *         - yul/precompiles/ecmul_source.yul
 *
 * Optimization level 3:
 * ---------------------
 *
 *     2588 hashes generated, 223/225 units compiled
 *     2 units failed to compile:
 *         - solidity/simple/immutable_evm
 *         - yul/precompiles/ecmul_source.yul
 *
 * Optimization level z:
 * ---------------------
 *
 *     2588 hashes generated, 223/225 units compiled
 *     2 units failed to compile:
 *         - solidity/simple/immutable_evm
 *         - yul/precompiles/ecmul_source.yul
 *
 * Total hashes: 7764
 * ```
 */
function buildReport(result, totalUnits, totalHashes, optLevels, outputFile) {
    /** @type {string[]} */
    const reportPerOptLevel = [];

    for (const optLevel of optLevels) {
        const hashCount = countHashes(result.hashes[optLevel]);
        const failedPathsAtOptLevel = Object.keys(result.failedPaths[optLevel]);
        const failedCount = failedPathsAtOptLevel.length;
        const successCount = totalUnits - failedCount;

        reportPerOptLevel.push(
            "",
            `Optimization level ${optLevel}:`,
            "---------------------",
            "",
            `    ${hashCount} hashes generated, ${successCount}/${totalUnits} units compiled`,
        );

        if (failedCount > 0) {
            reportPerOptLevel.push(`    ${failedCount} units failed to compile:`);
            for (const file of failedPathsAtOptLevel) {
                reportPerOptLevel.push(`        - ${file}`);
            }
        }
    }

    const report = [
        "",
        "===========================================",
        "SUMMARY",
        "===========================================",
        ...reportPerOptLevel,
        "",
        `Total hashes: ${totalHashes}`,
        totalHashes ? "" : "\n❌ FAILURE: No hashes were generated!\n",
        "For all hashes generated and failed compilation units, see uploaded artifact or:",
        `- ${outputFile})`,
        "",
        "===========================================",
    ];

    return report.join("\n");
}

/**
 * The main entry point.
 * Parses and validates arguments and initiates compilation, hashing, and final reporting.
 * @returns {Promise<void>}
 */
async function main() {
    // Node.js 20.12.0+ is required for recursive `fs.promises.readdir` with `parentPath` support.
    const [major, minor] = process.versions.node.split(".").map(Number);
    if (major < 20 || (major === 20 && minor < 12)) {
        throw new ValidationError(`Node.js 20.12.0+ required, found ${process.versions.node}`);
    }

    const {
        resolcPath,
        baseDir,
        projectsDirs,
        outputFile,
        optLevels,
        platformLabel,
        soljsonPath,
        debug,
        help,
    } = parseArguments();

    if (help) {
        console.log(getUsage());
        process.exit(0);
    }

    if (!fs.existsSync(resolcPath)) {
        throw new ValidationError(`resolc not found: ${resolcPath}`);
    }

    if (!fs.existsSync(baseDir)) {
        throw new ValidationError(`Base directory not found: ${baseDir}`);
    }

    for (const projectsDir of projectsDirs) {
        if (!fs.existsSync(projectsDir)) {
            throw new ValidationError(`Projects directory not found: ${projectsDir}`);
        }

        const relativePath = path.relative(baseDir, projectsDir);
        const isAboveBaseDir = relativePath.startsWith("..");
        const isOnDifferentDrive = path.isAbsolute(relativePath);
        if (isAboveBaseDir || isOnDifferentDrive) {
            throw new ValidationError([
                "Projects directory is not under the base directory:",
                `- Base directory:     ${baseDir}`,
                `- Projects directory: ${projectsDir}`,
            ].join("\n"));
        }
    }

    /** @type {(() => ResolcWasm) | null} */
    let createResolc = null;
    /** @type {unknown} */
    let soljson = null;

    const isWasm = !!soljsonPath;
    if (isWasm) {
        const hasJsExtension = [".js", ".mjs", ".cjs"].some(ext => resolcPath.endsWith(ext));
        if (!hasJsExtension) {
            console.warn([
                "Warning: soljson path provided but the resolc path doesn't seem to be a JavaScript file.",
                `  resolc: ${resolcPath}`,
                `  soljson: ${soljsonPath}`,
                '  For native builds, omit the soljson argument or use "".',
            ].join("\n"));
        }
        if (!fs.existsSync(soljsonPath)) {
            throw new ValidationError(`soljson not found: ${soljsonPath}`);
        }
        try {
            createResolc = require(resolcPath);
        } catch (error) {
            throw new ValidationError(`Failed to load resolc Node.js module \`${resolcPath}\`: ${error instanceof Error ? error.message : error}`);
        }
        try {
            soljson = require(soljsonPath);
        } catch (error) {
            throw new ValidationError(`Failed to load soljson Node.js module \`${soljsonPath}\`: ${error instanceof Error ? error.message : error}`);
        }
    } else {
        try {
            fs.accessSync(resolcPath, fs.constants.X_OK);
        } catch {
            throw new ValidationError(
                [
                    `resolc binary is not executable: ${resolcPath}`,
                    `If it is a native binary, run: chmod +x "${resolcPath}"`,
                    `If it is a JavaScript file for Wasm, provide the soljson path`,
                ].join("\n"),
                { showUsage: true }
            );
        }
    }

    for (const optLevel of optLevels) {
        if (!VALID_OPT_LEVELS.includes(optLevel)) {
            const errorPrefix = optLevel === ""
                ? "Please provide an optimization level"
                : `Invalid optimization level "${optLevel}"`;
            throw new ValidationError(`${errorPrefix}. Valid levels are: ${VALID_OPT_LEVELS.join(", ")}`, { showUsage: true });
        }
    }

    if (platformLabel === "") {
        throw new ValidationError("Please provide a non-empty platform label");
    }

    fs.mkdirSync(path.dirname(outputFile), { recursive: true });

    const [solidityUnits, yulUnits] = await Promise.all([
        loadCompilationUnits(baseDir, projectsDirs, Language.Solidity),
        loadCompilationUnits(baseDir, projectsDirs, Language.Yul),
    ]);

    const totalSolidityFiles = solidityUnits.reduce((sum, unit) => sum + unit.files.length, 0);
    const totalYulFiles = yulUnits.reduce((sum, unit) => sum + unit.files.length, 0);
    const totalFiles = totalSolidityFiles + totalYulFiles;
    const totalUnits = solidityUnits.length + yulUnits.length;

    console.log(`Found ${totalFiles} files (${totalUnits} compilation units)`);
    console.log(`  - Solidity: ${totalSolidityFiles} files (${solidityUnits.length} compilation units)`);
    console.log(`  - Yul: ${totalYulFiles} files (${yulUnits.length} compilation units)`);

    /** @type {CompileConfig} */
    const config = {
        resolcPath,
        optLevels,
        createResolc,
        soljson,
        debug,
    };
    const solidityResults = await compileAndHashAll(solidityUnits, Language.Solidity, config);
    const yulResults = await compileAndHashAll(yulUnits, Language.Yul, config);
    const { result, totalHashes } = aggregateResults(solidityResults.concat(yulResults), optLevels);

    writeResult(outputFile, platformLabel, result);
    const report = buildReport(result, totalUnits, totalHashes, optLevels, outputFile);

    if (totalHashes) {
        console.log(report);
    } else {
        console.error(report);
        process.exit(1);
    }
}

main().catch((error) => {
    if (error instanceof ValidationError) {
        console.error(error.message);
        if (error.showUsage) {
            console.error();
            console.error(getUsage());
        }
    } else {
        // Include the full stack trace for unexpected exceptions.
        console.error(error);
    }
    process.exit(1);
});
