#!/bin/bash

# This script compiles all `.sol` and `.yul` files from the provided contracts
# directory (and its subdirectories) using the provided resolc binary. For each
# compiled contract, it extracts the bytecode and computes a SHA256 hash. These
# hashes are stored in files per optimization level:
#       - <output-dir>/O0.txt
#           solidity/simple/loop/array/simple.sol:ContractName:<hash>
#           yul/instructions/byte.yul:ContractName:<hash>
#       - <output-dir>/O3.txt
#           solidity/simple/loop/array/simple.sol:ContractName:<hash>
#           yul/instructions/byte.yul:ContractName:<hash>
#       - <output-dir>/Oz.txt
#           solidity/simple/loop/array/simple.sol:ContractName:<hash>
#           yul/instructions/byte.yul:ContractName:<hash>
#
# Usage: compile-and-hash.sh <resolc-binary> <contracts-dir> <output-dir> [opt-levels]
#   opt-levels: Comma-separated optimization levels (default: "0,3,z")

set -euo pipefail

if [ $# -lt 3 ] || [ $# -gt 4 ]; then
    echo "Error: Expected 3 or 4 arguments, got $#"
    echo "Usage: $0 <resolc-binary> <contracts-dir> <output-dir> [opt-levels]"
    echo "  opt-levels: Comma-separated list (default: \"0,3,z\")"
    exit 1
fi

RESOLC="$1"
CONTRACTS_DIR="${2%/}"
OUTPUT_DIR="${3%/}"
OPT_LEVELS_STR="${4:-0,3,z}"
IFS=',' read -ra OPT_LEVELS <<< "$OPT_LEVELS_STR"

# Trim whitespace from each optimization level.
for i in "${!OPT_LEVELS[@]}"; do
    OPT_LEVELS[$i]="${OPT_LEVELS[$i]// /}"
done

if [ ! -f "$RESOLC" ]; then
    echo "Error: resolc binary not found: $RESOLC"
    exit 1
fi

if [ ! -d "$CONTRACTS_DIR" ]; then
    echo "Error: Contracts directory not found: $CONTRACTS_DIR"
    exit 1
fi

VALID_OPT_LEVELS="0 1 2 3 s z"
for opt in "${OPT_LEVELS[@]}"; do
    if [[ ! " $VALID_OPT_LEVELS " =~ " $opt " ]]; then
        echo "Error: Invalid optimization level \`$opt\`. Valid levels are: $VALID_OPT_LEVELS"
        exit 1
    fi
done

mkdir -p "$OUTPUT_DIR"

# For each optimization level, create an output file for hashes as well as
# for failed compilations if they do not exist, otherwise truncate them.
for opt in "${OPT_LEVELS[@]}"; do
    echo -n > "$OUTPUT_DIR/O${opt}.txt"
    echo -n > "$OUTPUT_DIR/failed_O${opt}.txt"
done

# Total number of files processed.
TOTAL_COUNT=0

# Compiles a single contract and stores its bytecode hash.
# Arguments:
#   $1 - File path to the contract.
#   $2 - Optimization level (0, 3, or z).
compile_and_hash_one() {
    local file_path="$1"
    local opt="$2"

    local is_yul="false"
    if [[ "$file_path" == *.yul ]]; then
        is_yul="true"
    fi

    # To ensure that the file path added as part of a hash entry is consistent, and thus
    # comparable, across platforms it is normalized (potential backslashes are replaced
    # with forward slashes) and converted to a path relative to the `CONTRACTS_DIR`.
    # Example:
    #     * CONTRACTS_DIR = /path/to/contracts/fixtures
    #     * file_path = /path/to/contracts/fixtures/solidity/simple/loop/array/simple.sol
    #     * relative_path = solidity/simple/loop/array/simple.sol
    local normalized_file_path="${file_path//\\//}"
    local normalized_base_dir="${CONTRACTS_DIR//\\//}"
    local relative_path="${normalized_file_path#$normalized_base_dir/}"

    # Compile the contract.
    local exit_code=0
    local output
    if [ "$is_yul" = "true" ]; then
        output=$($RESOLC -O${opt} --yul --bin "$file_path" 2>&1) || exit_code=$?
    else
        output=$($RESOLC -O${opt} --bin "$file_path" 2>&1) || exit_code=$?
    fi

    # Track files that failed to compile.
    if [ "$exit_code" -ne 0 ]; then
        echo "$relative_path" >> "$OUTPUT_DIR/failed_O${opt}.txt"
    fi

    # Parse each contract section from the output and compute hashes.
    # For each contract, output a line: <relative_path>:<ContractName>:<hash>
    #
    # Compilation output format:
    #   ======= <file>:<ContractName> =======
    #   Binary:
    #   <bytecode starting with 50564d>
    local contract_name=""
    while IFS= read -r line; do
        # If it is the header, extract the contract name.
        if [[ "$line" =~ \.(sol|yul):([^ ]+) ]]; then
            contract_name="${BASH_REMATCH[2]}"
        # If it is the bytecode, compute its hash.
        # (If a contract compiles on one platform but not another, the hash files will also reveal this mismatch.)
        elif [[ "$line" =~ ^50564d[0-9a-fA-F]+$ ]]; then
            if [ -n "$contract_name" ]; then
                local hash
                hash=$(echo -n "$line" | sha256sum | awk '{print $1}')
                echo "${relative_path}:${contract_name}:${hash}" >> "$OUTPUT_DIR/O${opt}.txt"
            fi
        fi
    done <<< "$output"
}

# Compiles all contracts of the given type and stores their bytecode hashes.
# Arguments:
#   $1 - File extension ("sol" or "yul").
#   $2 - Display label ("Solidity" or "Yul").
compile_and_hash_all() {
    local extension="$1"
    local label="$2"
    local count=0

    echo ""
    echo "=== Compiling $label contracts ==="
    while IFS= read -r -d '' file_path; do
        for opt in "${OPT_LEVELS[@]}"; do
            compile_and_hash_one "$file_path" "$opt"
        done
        count=$((count + 1))
        if [ $((count % 200)) -eq 0 ]; then
            echo "Processed $count files..."
        fi
    done < <(find "$CONTRACTS_DIR" -name "*.$extension" -print0 2>/dev/null | sort -z)
    echo "Total $label files processed: $count"

    TOTAL_COUNT=$((TOTAL_COUNT + count))
}

compile_and_hash_all "sol" "Solidity"
compile_and_hash_all "yul" "Yul"

# Sort hash files for deterministic comparison across platforms.
# This ensures that the same contracts appear in the same order
# regardless of filesystem ordering differences between platforms.
for opt in "${OPT_LEVELS[@]}"; do
    sort -o "$OUTPUT_DIR/O${opt}.txt" "$OUTPUT_DIR/O${opt}.txt"
done

# Show final summary.
# Example output:
#   Optimization level: O0
#       160 hashes generated, 143/145 files compiled
#       2 files failed to compile:
#         - solidity/simple/loop/array/simple.sol
#         - yul/instructions/byte.yul
#
#   Optimization level: O3
#       161 hashes generated, 144/145 files compiled
#       1 files failed to compile:
#         - solidity/simple/loop/array/simple.sol
#
#   Optimization level: Oz
#       162 hashes generated, 145/145 files compiled
echo ""
echo "==========================================="
echo "SUMMARY"
echo "==========================================="
for opt in "${OPT_LEVELS[@]}"; do
    HASH_COUNT=$(($(wc -l < "$OUTPUT_DIR/O${opt}.txt")))
    FAILED_COUNT=$(($(wc -l < "$OUTPUT_DIR/failed_O${opt}.txt")))
    SUCCESSFUL_COUNT=$((TOTAL_COUNT - FAILED_COUNT))

    echo ""
    echo "Optimization level: O${opt}"
    echo "    $HASH_COUNT hashes generated, $SUCCESSFUL_COUNT/$TOTAL_COUNT files compiled"
    if [ "$FAILED_COUNT" -gt 0 ]; then
        echo "    $FAILED_COUNT files failed to compile:"
        sort "$OUTPUT_DIR/failed_O${opt}.txt" | sed 's/^/      - /'
        echo ""
    fi
done
echo ""
echo "==========================================="
