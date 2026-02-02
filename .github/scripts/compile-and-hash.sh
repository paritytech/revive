#!/bin/bash

# This script compiles all `.sol` and `.yul` files from the provided Solidity
# and Yul directories (and their subdirectories) respectively, using the provided
# resolc binary. For each compiled contract, it extracts the bytecode and computes
# a SHA256 hash. These hashes are stored in files per optimization level:
#       - <output-dir>/O0.txt
#           simple/loop/array/simple.sol:ContractName:<hash>
#           instructions/byte.yul:ContractName:<hash>
#       - <output-dir>/O3.txt
#           simple/loop/array/simple.sol:ContractName:<hash>
#           instructions/byte.yul:ContractName:<hash>
#       - <output-dir>/Oz.txt
#           simple/loop/array/simple.sol:ContractName:<hash>
#           instructions/byte.yul:ContractName:<hash>
#
# Usage: compile-and-hash.sh <resolc-binary> <solidity-contracts-dir> <yul-contracts-dir> <output-dir>

set -euo pipefail

if [ $# -ne 4 ]; then
    echo "Error: Expected 4 arguments, got $#"
    echo "Usage: $0 <resolc-binary> <solidity-contracts-dir> <yul-contracts-dir> <output-dir>"
    exit 1
fi

RESOLC="$1"
SOLIDITY_CONTRACTS_DIR="${2%/}"
YUL_CONTRACTS_DIR="${3%/}"
OUTPUT_DIR="${4%/}"

if [ ! -f "$RESOLC" ]; then
    echo "Error: resolc binary not found: $RESOLC"
    exit 1
fi

if [ ! -d "$SOLIDITY_CONTRACTS_DIR" ]; then
    echo "Error: Solidity contracts directory not found: $SOLIDITY_CONTRACTS_DIR"
    exit 1
fi

if [ ! -d "$YUL_CONTRACTS_DIR" ]; then
    echo "Error: Yul contracts directory not found: $YUL_CONTRACTS_DIR"
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
#   $3 - "true" if Yul contract, "false" if Solidity.
compile_and_hash() {
    local file_path="$1"
    local opt="$2"
    local is_yul="$3"

    local base_dir
    if [ "$is_yul" = "true" ]; then
        base_dir="$YUL_CONTRACTS_DIR"
    else
        base_dir="$SOLIDITY_CONTRACTS_DIR"
    fi

    # To ensure that the file path added as part of a hash entry is consistent, and thus
    # comparable, across platforms it is normalized (potential backslashes are replaced
    # with forward slashes) and converted to a path relative to the `base_dir`.
    # Example:
    #     * base_dir = /path/to/contracts/fixtures/solidity
    #     * file_path = /path/to/contracts/fixtures/solidity/simple/loop/array/simple.sol
    #     * relative_path = simple/loop/array/simple.sol
    local normalized_file_path="${file_path//\\//}"
    local normalized_base_dir="${base_dir//\\//}"
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
    # Output format:
    #   ======= <file>:<ContractName> =======
    #   Binary:
    #   <bytecode starting with 50564d>
    #
    # For each contract, output a line: <relative_path>:<ContractName>:<hash>
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
while IFS= read -r -d '' file; do
    for opt in 0 3 z; do
        compile_and_hash "$file" "$opt" "false"
    done
    ((++SOLIDITY_COUNT))
    if [ $((SOLIDITY_COUNT % 200)) -eq 0 ]; then
        echo "Processed $SOLIDITY_COUNT files..."
    fi
done < <(find "$SOLIDITY_CONTRACTS_DIR" -name "*.sol" -print0 2>/dev/null | sort -z)
echo "Total Solidity files compiled: $SOLIDITY_COUNT"

# Compile all Yul contracts.
echo ""
echo "=== Compiling Yul contracts ==="
YUL_COUNT=0
while IFS= read -r -d '' file; do
    for opt in 0 3 z; do
        compile_and_hash "$file" "$opt" "true"
    done
    ((++YUL_COUNT))
    if [ $((YUL_COUNT % 200)) -eq 0 ]; then
        echo "Processed $YUL_COUNT files..."
    fi
done < <(find "$YUL_CONTRACTS_DIR" -name "*.yul" -print0 2>/dev/null | sort -z)
echo "Total Yul files compiled: $YUL_COUNT"

# Sort hash files for deterministic comparison across platforms.
# This ensures that the same contracts appear in the same order
# regardless of filesystem ordering differences between platforms.
for opt in O0 O3 Oz; do
    sort -o "$OUTPUT_DIR/${opt}.txt" "$OUTPUT_DIR/${opt}.txt"
done

# Show final summary.
# Example output:
#   O0: 120 hashes generated, 143/145 files compiled
#       2 files failed to compile:
#         - simple/loop/array/simple.sol
#         - instructions/byte.yul
#
#   O3: 121 hashes generated, 144/145 files compiled
#       1 files failed to compile:
#         - simple/loop/array/simple.sol
#
#   Oz: 122 hashes generated, 145/145 files compiled
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

    echo "${opt}: $HASH_COUNT hashes generated, $SUCCESSFUL_COUNT/$TOTAL_COUNT files compiled"
    if [ "$FAILED_COUNT" -gt 0 ]; then
        echo "    $FAILED_COUNT files failed to compile:"
        echo -e "$FAILED" | grep . | sort | sed 's/^/      - /'
        echo ""
    fi
done
echo "==========================================="
