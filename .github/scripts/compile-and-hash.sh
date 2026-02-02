#!/bin/bash

# This script compiles all `.sol` and `.yul` files from the provided Solidity
# and Yul directories (and their subdirectories) respectively, using the provided
# resolc binary. For each compiled contract, it extracts the bytecode and computes
# a SHA256 hash. These hashes are stored in files per optimization level:
#       - <output-dir>/O0.txt
#           solidity/simple/loop/array/simple.sol:<hash>
#           yul/instructions/byte.yul:<hash>
#       - <output-dir>/O3.txt
#           solidity/simple/loop/array/simple.sol:<hash>
#           yul/instructions/byte.yul:<hash>
#       - <output-dir>/Oz.txt
#           solidity/simple/loop/array/simple.sol:<hash>
#           yul/instructions/byte.yul:<hash>
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
# Each file will later contain lines in the format: "relative/path/to/contract.sol:hash"
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
    local file="$1"
    local opt="$2"
    local is_yul="$3"

    local prefix
    local base_dir
    if [ "$is_yul" = "true" ]; then
        prefix="yul"
        base_dir="$YUL_CONTRACTS_DIR"
    else
        prefix="solidity"
        base_dir="$SOLIDITY_CONTRACTS_DIR"
    fi

    # Convert to relative path with prefix for consistent naming across platforms.
    # Example:
    #     * base_dir = /path/to/contracts/fixtures/solidity
    #     * file = /path/to/contracts/fixtures/solidity/simple/loop/array/simple.sol
    #     * relative_path = solidity/simple/loop/array/simple.sol
    local relative_path="${prefix}/${file#$base_dir/}"

    # Compile the contract.
    local exit_code=0
    if [ "$is_yul" = "true" ]; then
        OUTPUT=$($RESOLC -O${opt} --yul --bin "$file" 2>&1) || exit_code=$?
    else
        OUTPUT=$($RESOLC -O${opt} --bin "$file" 2>&1) || exit_code=$?
    fi

    # Track contracts that failed to compile.
    if [ "$exit_code" -ne 0 ]; then
        case "$opt" in
            0) FAILED_O0="${FAILED_O0}${relative_path}\n" ;;
            3) FAILED_O3="${FAILED_O3}${relative_path}\n" ;;
            z) FAILED_Oz="${FAILED_Oz}${relative_path}\n" ;;
        esac
    fi

    # Extract the bytecode from the output by taking the first occurrence of the
    # pattern starting with "50564d" (the starting magic bytes which is "PVM").
    BYTECODE=$(echo "$OUTPUT" | grep -oE '50564d[0-9a-fA-F]+' | head -1 || true)

    # If compilation succeeded and produced valid bytecode, calculate and store its hash.
    # If a contract compiles on one platform but not another, the hash files will store this mismatch.
    if [ -n "$BYTECODE" ]; then
        HASH=$(echo -n "$BYTECODE" | sha256sum | awk '{print $1}')
        echo "${relative_path}:${HASH}" >> "$OUTPUT_DIR/O${opt}.txt"
    fi
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
        echo "Processed $SOLIDITY_COUNT contracts..."
    fi
done < <(find "$SOLIDITY_CONTRACTS_DIR" -name "*.sol" -print0 2>/dev/null | sort -z)
echo "Total Solidity contracts compiled: $SOLIDITY_COUNT"

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
        echo "Processed $YUL_COUNT contracts..."
    fi
done < <(find "$YUL_CONTRACTS_DIR" -name "*.yul" -print0 2>/dev/null | sort -z)
echo "Total Yul contracts compiled: $YUL_COUNT"

# Sort hash files for deterministic comparison across platforms.
# This ensures that the same contracts appear in the same order
# regardless of filesystem ordering differences between platforms.
for opt in O0 O3 Oz; do
    sort -o "$OUTPUT_DIR/${opt}.txt" "$OUTPUT_DIR/${opt}.txt"
done

# Show final summary.
# Example output:
#   O0: 120 hashes generated, 143/145 contracts compiled
#       2 contracts failed to compile:
#         - solidity/simple/loop/array/simple.sol
#         - yul/instructions/byte.yul
#
#   O3: 121 hashes generated, 144/145 contracts compiled
#       1 contracts failed to compile:
#         - solidity/simple/loop/array/simple.sol
#
#   Oz: 122 hashes generated, 145/145 contracts compiled
echo ""
echo "==========================================="
echo "SUMMARY"
echo "==========================================="
TOTAL_COUNT=$((SOLIDITY_COUNT + YUL_COUNT))
for opt in O0 O3 Oz; do
    HASH_COUNT=$(grep -c ':' "$OUTPUT_DIR/${opt}.txt") || HASH_COUNT=0
    case "$opt" in
        O0) FAILED="$FAILED_O0" ;;
        O3) FAILED="$FAILED_O3" ;;
        Oz) FAILED="$FAILED_Oz" ;;
    esac
    FAILED_COUNT=$(echo -e "$FAILED" | grep -c .) || FAILED_COUNT=0
    SUCCESSFUL_COUNT=$((TOTAL_COUNT - FAILED_COUNT))

    echo "${opt}: $HASH_COUNT hashes generated, $SUCCESSFUL_COUNT/$TOTAL_COUNT contracts compiled"
    if [ "$FAILED_COUNT" -gt 0 ]; then
        echo "    $FAILED_COUNT contracts failed to compile:"
        echo -e "$FAILED" | grep . | sort | sed 's/^/      - /'
        echo ""
    fi
done
echo "==========================================="
