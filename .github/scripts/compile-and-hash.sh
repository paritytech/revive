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
# Usage: compile-and-hash.sh <resolc-binary> <contracts-dir> <output-dir>

set -euo pipefail

if [ $# -ne 3 ]; then
    echo "Error: Expected 3 arguments, got $#"
    echo "Usage: $0 <resolc-binary> <contracts-dir> <output-dir>"
    exit 1
fi

RESOLC="$1"
CONTRACTS_DIR="${2%/}"
OUTPUT_DIR="${3%/}"

if [ ! -f "$RESOLC" ]; then
    echo "Error: resolc binary not found: $RESOLC"
    exit 1
fi

if [ ! -d "$CONTRACTS_DIR" ]; then
    echo "Error: Contracts directory not found: $CONTRACTS_DIR"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

# Create an output file for each optimization level if it does not exist, otherwise truncate it.
for opt in O0 O3 Oz; do
    echo -n > "$OUTPUT_DIR/${opt}.txt"
done

# Contracts that failed to compile per optimization level.
FAILED_O0=""
FAILED_O3=""
FAILED_Oz=""

# Compiles a single contract and stores its bytecode hash.
# Arguments:
#   $1 - File path to the contract.
#   $2 - Optimization level (0, 3, or z).
compile_and_hash() {
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

    # Track contracts that failed to compile.
    if [ "$exit_code" -ne 0 ]; then
        case "$opt" in
            0) FAILED_O0="${FAILED_O0}${relative_path}\n" ;;
            3) FAILED_O3="${FAILED_O3}${relative_path}\n" ;;
            z) FAILED_Oz="${FAILED_Oz}${relative_path}\n" ;;
        esac
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

# Compile all Solidity contracts.
echo "=== Compiling Solidity contracts ==="
SOLIDITY_COUNT=0
while IFS= read -r -d '' file_path; do
    for opt in 0 3 z; do
        compile_and_hash "$file_path" "$opt"
    done
    ((++SOLIDITY_COUNT))
    if [ $((SOLIDITY_COUNT % 200)) -eq 0 ]; then
        echo "Processed $SOLIDITY_COUNT files..."
    fi
done < <(find "$CONTRACTS_DIR" -name "*.sol" -print0 2>/dev/null | sort -z)
echo "Total Solidity files compiled: $SOLIDITY_COUNT"

# Compile all Yul contracts.
echo ""
echo "=== Compiling Yul contracts ==="
YUL_COUNT=0
while IFS= read -r -d '' file_path; do
    for opt in 0 3 z; do
        compile_and_hash "$file_path" "$opt"
    done
    ((++YUL_COUNT))
    if [ $((YUL_COUNT % 200)) -eq 0 ]; then
        echo "Processed $YUL_COUNT files..."
    fi
done < <(find "$CONTRACTS_DIR" -name "*.yul" -print0 2>/dev/null | sort -z)
echo "Total Yul files compiled: $YUL_COUNT"

# Sort hash files for deterministic comparison across platforms.
# This ensures that the same contracts appear in the same order
# regardless of filesystem ordering differences between platforms.
for opt in O0 O3 Oz; do
    sort -o "$OUTPUT_DIR/${opt}.txt" "$OUTPUT_DIR/${opt}.txt"
done

# Show final summary.
# Example output:
#   Optimization level: O0
#       120 hashes generated, 143/145 files compiled
#       2 files failed to compile:
#         - solidity/simple/loop/array/simple.sol
#         - yul/instructions/byte.yul
#
#   Optimization level: O3
#       121 hashes generated, 144/145 files compiled
#       1 files failed to compile:
#         - solidity/simple/loop/array/simple.sol
#
#   Optimization level: Oz
#       122 hashes generated, 145/145 files compiled
echo ""
echo "==========================================="
echo "SUMMARY"
echo "==========================================="
TOTAL_COUNT=$((SOLIDITY_COUNT + YUL_COUNT))
for opt in O0 O3 Oz; do
    HASH_COUNT=$(($(wc -l < "$OUTPUT_DIR/${opt}.txt")))
    case "$opt" in
        O0) FAILED="$FAILED_O0" ;;
        O3) FAILED="$FAILED_O3" ;;
        Oz) FAILED="$FAILED_Oz" ;;
    esac
    FAILED_COUNT=$(echo -e "$FAILED" | grep -c .) || FAILED_COUNT=0
    SUCCESSFUL_COUNT=$((TOTAL_COUNT - FAILED_COUNT))

    echo ""
    echo "Optimization level: ${opt}"
    echo "    $HASH_COUNT hashes generated, $SUCCESSFUL_COUNT/$TOTAL_COUNT files compiled"
    if [ "$FAILED_COUNT" -gt 0 ]; then
        echo "    $FAILED_COUNT files failed to compile:"
        echo -e "$FAILED" | grep . | sort | sed 's/^/      - /'
        echo ""
    fi
done
echo "==========================================="
