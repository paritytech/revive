#!/usr/bin/env node

// @ts-check

/**
 * This script compiles all `.sol` and `.yul` files from the provided contracts
 * directory (and its subdirectories) using the provided resolc (either native
 * binary or Wasm) and generates SHA256 hashes of each contract's bytecode.
 *
 * This script handles both native binaries and Wasm builds with shared logic.
 * Wasm mode is enabled when soljson is provided.
 *
 * ```
 * Usage: node compile-and-hash.mjs <resolc> <contracts-dir> <output-file> <opt-levels> <platform-label> [soljson] [debug]
 *
 *   resolc:         Path to native binary or Node.js module for Wasm
 *   contracts-dir:  Directory containing .sol (Solidity) and/or .yul (Yul) files
 *   output-file:    File path to write the output JSON to (the file and parent directories are created automatically)
 *   opt-levels:     Comma-separated optimization levels (e.g., "0,3,z")
 *   platform-label: Label for the platform (e.g., linux, macos, windows, wasm)
 *   soljson:        Path to soljson for Wasm builds (omit for native or use "")
 *   debug:          "true" or "1" to enable verbose output (default: "false")
 * ```
 *
 * Output format:
 * - The file paths used as keys are normalized for cross-platform comparison.
 *   They should thereby not be used to access the filesystem.
 *
 * ```json
 * {
 *   "platform": "linux",
 *   "hashes": {
 *     "0": {
 *       "solidity/simple/loop/array/simple.sol": {
 *         "ContractNameA": "<sha256>",
 *         "ContractNameB": "<sha256>"
 *       },
 *       "yul/instructions/byte.yul": {
 *         "ContractNameA": "<sha256>",
 *         "ContractNameB": "<sha256>"
 *       }
 *     },
 *     "3": { ... },
 *     "z": { ... }
 *   },
 *   "failedFiles": {
 *     "0": {
 *       "path/to/failed.sol": [
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
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

/**
 * Allows loading CommonJS modules (e.g. resolc.js, soljson.js) from ES modules.
 */
const require = createRequire(import.meta.url);

const VALID_OPT_LEVELS = ["0", "1", "2", "3", "s", "z"];
const PVM_BYTECODE_PREFIX = "50564d";
const DEFAULT_CONCURRENCY = os.availableParallelism?.() ?? os.cpus().length;

const Language = Object.freeze({
    Solidity: "Solidity",
    Yul: "Yul",
});

/**
 * @typedef {typeof Language[keyof typeof Language]} LanguageKind
 */

/**
 * @typedef {Object} SourceFile
 * @property {string} path - The platform-specfic file path.
 * @property {string} content - The file content.
 */

/**
 * @typedef {Object} Contract
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
 * @property {{[optLevel: string]: {[path: string]: HashEntry}}} hashes - The hashes for each path keyed by optimization level.
 * @property {{[optLevel: string]: {[path: string]: string[]}}} failedFiles - The normalized paths of the files that failed to compile and their errors, keyed by optimization level.
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
 * @property {string} contractsDir - The base directory for the contracts to compile.
 * @property {string[]} optLevels - The optimization levels to compile with.
 * @property {boolean} isWasm - Whether the Wasm build should be used.
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
 * @returns {string} The usage for the script.
 */
function getUsage() {
    return [
        "Usage: node compile-and-hash.mjs <resolc> <contracts-dir> <output-file> <opt-levels> <platform-label> [soljson] [debug]",
        "  resolc:         Path to native binary or Node.js module for Wasm",
        "  contracts-dir:  Directory containing .sol (Solidity) and/or .yul (Yul) files",
        "  output-file:    Path to write the output JSON file (parent directories created automatically)",
        "  opt-levels:     Comma-separated optimization levels (e.g., \"0,3,z\")",
        "  platform-label: Label for the platform (e.g., linux, macos, windows, wasm)",
        '  soljson:        Path to soljson for Wasm builds (omit for native or use "")',
        '  debug:          "true" or "1" to enable verbose output (default: "false")',
    ].join("\n");
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
 * Reads a file and returns its platform-specific path and content.
 * @param {string} filePath - The file path to read.
 * @returns {Promise<SourceFile>} The file with its content.
 */
async function readFile(filePath) {
    return {
        path: filePath,
        content: await fs.promises.readFile(filePath, "utf-8"),
    };
}

/**
 * Recursively finds all files with the given extension in a directory.
 * Requires Node.js 20.12.0+ for recursive `readdir` with `parentPath` support.
 * @param {string} directory - The directory to search in.
 * @param {string} extension - The file extension to match (e.g., ".sol").
 * @returns {Promise<string[]>} Normalized file paths in platform-specific format.
 */
async function findFiles(directory, extension) {
    const entries = await fs.promises.readdir(directory, { recursive: true, withFileTypes: true });
    return entries
        .filter(entry => entry.isFile() && path.extname(entry.name) === extension)
        .map(entry => path.join(entry.parentPath, entry.name));
}

/**
 * Converts `filePath` to a relative path from `baseDir` and normalizes it with forward slashes.
 * This ensures that the file path added as part of a hash entry is consistent, and thus
 * comparable, across platforms. It should thereby not be used to access the filesystem.
 * @param {string} baseDir - The base directory to derive the relative path from.
 * @param {string} filePath - The file path.
 * @returns {string} The relative path with forward slashes.
 *
 * @example
 * ```
 * baseDir = /path/to/contracts/fixtures
 * filePath = /path/to/contracts/fixtures/solidity/simple/loop/array/simple.sol
 * normalizedPathId = solidity/simple/loop/array/simple.sol
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
        failedFiles: result.failedFiles,
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
 * @param {SourceFile} file - The source file.
 * @param {LanguageKind} language - The source code language.
 * @param {string} optLevel - The optimization level (e.g., "0", "3", "z").
 * @returns {Object} The standard JSON input object.
 */
function createStandardJsonInput(file, language, optLevel) {
    return {
        language,
        sources: {
            [file.path]: { content: file.content },
        },
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
    for (const [, fileContracts] of Object.entries(parsed.contracts || {})) {
        for (const [name, contract] of Object.entries(fileContracts)) {
            const bytecode = contract.evm?.bytecode?.object;
            if (bytecode?.startsWith(PVM_BYTECODE_PREFIX)) {
                contracts.push({ name, bytecode });
            }
        }
    }

    const errors = (parsed.errors || [])
        .filter((error) => error.severity === "error")
        .map((error) => error.formattedMessage || error.message);

    return { contracts, errors };
}

/**
 * Compiles a file using the native resolc binary.
 * @param {string} resolcBinary - The path to the resolc binary.
 * @param {SourceFile} file - The source file to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {string} optLevel - The optimization level (e.g., "0", "3", "z").
 * @returns {Promise<CompilationResult>} The compilation result.
 * @throws {Error} For system-level errors.
 */
async function compileNative(resolcBinary, file, language, optLevel) {
    const input = createStandardJsonInput(file, language, optLevel);

    // In standard JSON mode, compilation failures are reported as part
    // of the JSON output and will exit in a success state. System-level
    // errors will throw exceptions which are intentionally bubbled up.
    const promise = execFileAsync(resolcBinary, ["--standard-json"], {
        // 20-second timeout per compilation.
        timeout: 20_000,
        // 10MB buffer for large outputs.
        maxBuffer: 10 * 1024 * 1024,
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
 * Compiles a file using the Node module for Wasm.
 * @param {() => ResolcWasm} createResolc - The Wasm resolc factory.
 * @param {unknown} soljson - The soljson module.
 * @param {SourceFile} file - The source file to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {string} optLevel - The optimization level (e.g., "0", "3", "z").
 * @returns {CompilationResult} The compilation result.
 * @throws {Error} For system-level errors.
 */
function compileWasm(createResolc, soljson, file, language, optLevel) {
    const compiler = createResolc();
    compiler.soljson = soljson;

    const input = createStandardJsonInput(file, language, optLevel);
    compiler.writeToStdin(JSON.stringify(input));

    // In standard JSON mode, compilation failures are reported as part
    // of the JSON output and will exit in a success state. System-level
    // errors cause a non-zero exit code returned which should be thrown.
    const exitCode = compiler.callMain(["--standard-json"]);

    if (exitCode !== 0) {
        const stderr = compiler.readFromStderr();
        throw new Error(`Compiling ${file.path} exited with code ${exitCode}: ${stderr}`);
    }

    return parseStandardJsonOutput(compiler.readFromStdout());
}

/**
 * Compiles a single file at all optimization levels provided and hashes the bytecode.
 * Each hash entry has the format {@link HashEntry}.
 * @param {SourceFile} file - The source file to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {CompileConfig} config - The compilation configuration.
 * @returns {Promise<HashResult>} Hashes and files that failed to compile for each optimization level.
 */
async function compileAndHashOne(file, language, config) {
    const { resolcPath, contractsDir, optLevels, isWasm, createResolc, soljson, debug } = config;

    if (debug) {
        console.log(`[DEBUG] Compiling: ${file.path}`);
    }

    const normalizedPathId = getNormalizedPathId(contractsDir, file.path);

    /** @type {HashResult} */
    const result = {
        hashes: {},
        failedFiles: {},
    };

    for (const optLevel of optLevels) {
        result.hashes[optLevel] = {};
        result.failedFiles[optLevel] = {};

        const { contracts, errors } = isWasm
            ? compileWasm(/** @type {() => ResolcWasm} */(createResolc), soljson, file, language, optLevel)
            : await compileNative(resolcPath, file, language, optLevel);

        if (errors.length > 0) {
            if (debug) {
                console.warn(`[DEBUG] Error compiling \`${file.path}\` at optimization \`${optLevel}\`:\n- ${errors.join("\n- ")}`);
            }
            result.failedFiles[optLevel][normalizedPathId] = errors;
        }

        for (const { name, bytecode } of contracts) {
            if (!result.hashes[optLevel][normalizedPathId]) {
                result.hashes[optLevel][normalizedPathId] = {};
            }
            result.hashes[optLevel][normalizedPathId][name] = sha256(bytecode);
        }
    }

    return result;
}

/**
 * Compiles all provided files at all optimization levels provided and hashes the bytecode.
 * @param {string[]} filePaths - The paths to the files to compile.
 * @param {LanguageKind} language - The source code language.
 * @param {CompileConfig} config - The compilation configuration.
 * @returns {Promise<HashResult[]>} Per-file hash results.
 */
async function compileAndHashAll(filePaths, language, config) {
    console.log();
    console.log(`=== Compiling ${filePaths.length} ${language} files (batch size: ${DEFAULT_CONCURRENCY}) ===`);

    const files = await Promise.all(filePaths.map(readFile));

    // Sort by path (byte-order, not `localCompare()`) for deterministic batch-level processing order, helpful for debugging.
    files.sort((a, b) => Number(a.path > b.path) - Number(a.path < b.path));

    const PROGRESS_REPORT_INTERVAL = 200;
    let previousInterval = 0;
    /** @type {(numFilesProcessed: number) => void} */
    const reportProgress = (numFilesProcessed) => {
        if (numFilesProcessed === files.length) {
            console.log();
            return console.log(`Total ${language} files processed: ${files.length}`);
        }
        const currentInterval = Math.floor(numFilesProcessed / PROGRESS_REPORT_INTERVAL) * PROGRESS_REPORT_INTERVAL;
        if (currentInterval > previousInterval) {
            console.log(`Processed ${currentInterval} files...`);
            previousInterval = currentInterval;
        }
    };

    /** @param {SourceFile} file */
    const compile = (file) => compileAndHashOne(file, language, config);

    return batch(compile, files, DEFAULT_CONCURRENCY, reportProgress);
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
 * @param {(item: T) => Promise<R>} processor - The function to process each item.
 * @param {T[]} items - The items to process.
 * @param {number} batchSize - The number of items to process in parallel per batch.
 * @param {((numItemsProcessed: number) => void)|undefined} onBatchComplete - A callback called after each completed batch with the cumulative count processed.
 * @returns {Promise<R[]>} The results from all processed items.
 */
async function batch(processor, items, batchSize, onBatchComplete) {
    /** @type {R[]} */
    const result = [];
    for (let i = 0; i < items.length; i += batchSize) {
        const currentBatch = items.slice(i, i + batchSize);
        const batchResult = await Promise.all(currentBatch.map(processor));
        result.push(...batchResult);

        if (typeof onBatchComplete === "function") {
            const numItemsProcessed = Math.min(i + batchSize, items.length);
            onBatchComplete(numItemsProcessed);
        }
    }

    return result;
}

/**
 * Aggregates multiple per-file compilation and hash results into a single result.
 * @param {HashResult[]} results - The per-file compilation and hash results to aggregate.
 * @param {string[]} optLevels - The optimization levels used during compilation.
 * @returns {{result: HashResult, totalHashes: number}} Combined hash results and the total number of hashes generated.
 */
function aggregateResults(results, optLevels) {
    /** @type {HashResult} */
    const aggregatedResult = {
        hashes: {},
        failedFiles: {},
    };
    let totalHashes = 0;

    for (const optLevel of optLevels) {
        aggregatedResult.hashes[optLevel] = {};
        aggregatedResult.failedFiles[optLevel] = {};

        for (const partialResult of results) {
            Object.assign(aggregatedResult.hashes[optLevel], partialResult.hashes[optLevel]);
            Object.assign(aggregatedResult.failedFiles[optLevel], partialResult.failedFiles[optLevel]);
        }

        totalHashes += countHashes(aggregatedResult.hashes[optLevel]);
    }

    return { result: aggregatedResult, totalHashes };
}

/**
 * Builds a final report from the results.
 * @param {HashResult} result - The aggregated compilation and hash result.
 * @param {number} totalFilesProcessed - The total number of files processed.
 * @param {number} totalHashes - The total number of hashes generated.
 * @param {string[]} optLevels - The optimization levels used during compilation.
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
 *     160 hashes generated, 143/145 files compiled
 *     2 files failed to compile:
 *         - solidity/simple/loop/array/simple.sol
 *         - yul/instructions/byte.yul
 *
 * Optimization level 3:
 * ---------------------
 *
 *     161 hashes generated, 144/145 files compiled
 *     1 files failed to compile:
 *         - solidity/simple/loop/array/simple.sol
 *
 * Optimization level z:
 * ---------------------
 *
 *     162 hashes generated, 145/145 files compiled
 *
 * Total hashes: 483
 * ```
 */
function buildReport(result, totalFilesProcessed, totalHashes, optLevels) {
    /** @type {string[]} */
    const reportPerOptLevel = [];

    for (const optLevel of optLevels) {
        const hashCount = countHashes(result.hashes[optLevel]);
        const failedFilesAtOptLevel = Object.keys(result.failedFiles[optLevel]);
        const failedCount = failedFilesAtOptLevel.length;
        const successCount = totalFilesProcessed - failedCount;

        reportPerOptLevel.push(
            "",
            `Optimization level ${optLevel}:`,
            "---------------------",
            "",
            `    ${hashCount} hashes generated, ${successCount}/${totalFilesProcessed} files compiled`,
        );

        if (failedCount > 0) {
            reportPerOptLevel.push(`    ${failedCount} files failed to compile:`);
            for (const file of failedFilesAtOptLevel) {
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

    const args = process.argv.slice(2);

    if (args.includes("--help") || args.includes("-h")) {
        console.log(getUsage());
        process.exit(0);
    }

    if (args.length < 5 || args.length > 7) {
        throw new ValidationError(`Received an invalid number of arguments, got ${args.length}`, { showUsage: true });
    }

    const resolcPath = path.resolve(args[0]);
    const contractsDir = path.resolve(args[1]);
    const outputFile = path.resolve(args[2]);
    const optLevels = [...new Set(args[3].split(",").map((s) => s.trim()))];
    const platformLabel = args[4];
    const soljsonPath = args[5] ? path.resolve(args[5]) : null;
    const debug = ["true", "1"].includes(args[6]?.toLowerCase());

    if (!fs.existsSync(resolcPath)) {
        throw new ValidationError(`resolc not found: ${resolcPath}`);
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

    if (!fs.existsSync(contractsDir)) {
        throw new ValidationError(`Contracts directory not found: ${contractsDir}`);
    }

    for (const optLevel of optLevels) {
        if (!VALID_OPT_LEVELS.includes(optLevel)) {
            const errorPrefix = optLevel === ""
                ? "Please provide an optimization level"
                : `Invalid optimization level "${optLevel}"`;
            throw new ValidationError(`${errorPrefix}. Valid levels are: ${VALID_OPT_LEVELS.join(", ")}`, { showUsage: true });
        }
    }

    if (!platformLabel) {
        throw new ValidationError("Please provide a non-empty platform label");
    }

    fs.mkdirSync(path.dirname(outputFile), { recursive: true });

    const [solidityFiles, yulFiles] = await Promise.all([
        findFiles(contractsDir, ".sol"),
        findFiles(contractsDir, ".yul"),
    ]);
    const totalFiles = solidityFiles.length + yulFiles.length;
    console.log(`Found ${totalFiles} files (${solidityFiles.length} Solidity, ${yulFiles.length} Yul)`);

    /** @type {CompileConfig} */
    const config = {
        resolcPath,
        contractsDir,
        optLevels,
        isWasm,
        createResolc,
        soljson,
        debug,
    };
    const solidityResults = await compileAndHashAll(solidityFiles, Language.Solidity, config);
    const yulResults = await compileAndHashAll(yulFiles, Language.Yul, config);
    const { result, totalHashes } = aggregateResults(solidityResults.concat(yulResults), optLevels);

    writeResult(outputFile, platformLabel, result);
    const report = buildReport(result, totalFiles, totalHashes, optLevels);

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
