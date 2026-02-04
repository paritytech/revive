#!/bin/bash

# This script compares bytecode hashes across the provided platforms
# to verify that resolc generates identical bytecode. Hashes are
# compared for each optimization level provided.
#
# Usage: compare-hashes.sh <hashes-dir> <opt-levels> [platform-prefix]
#   opt-levels: Comma-separated optimization levels (e.g., "0,3,z")
#
# The provided hashes directory should contain subdirectories for each platform
# to compare (e.g., linux-musl, macos-universal, windows) each of which containing
# files per optimization level (O0.txt, O3.txt, Oz.txt) storing the hashes
# (the script `compile-and-hash.sh` can generate such files):
#       - <platform>
#           - O0.txt
#               <path1>:<ContractName1>:<hash>
#               <path2>:<ContractName2>:<hash>
#           - O3.txt
#               <path1>:<ContractName1>:<hash>
#               <path2>:<ContractName2>:<hash>
#           - Oz.txt
#               <path1>:<ContractName1>:<hash>
#               <path2>:<ContractName2>:<hash>
#
# If the platform subdirectory names have a prefix (e.g. "hashes-" in "hashes-linux-musl"),
# the prefix can be provided in order for outputs to show the platform name without the prefix.

set -euo pipefail

if [ $# -lt 2 ] || [ $# -gt 3 ]; then
    echo "Error: Expected 2 or 3 arguments, got $#"
    echo "Usage: $0 <hashes-dir> <opt-levels> [platform-prefix]"
    echo "  opt-levels: Comma-separated list (e.g., \"0,3,z\")"
    exit 1
fi

HASHES_DIR="${1%/}"
OPT_LEVELS_STR="$2"
IFS=',' read -ra OPT_LEVELS <<< "$OPT_LEVELS_STR"
PLATFORM_PREFIX="${3:-}"

# Trim whitespace from each optimization level.
for i in "${!OPT_LEVELS[@]}"; do
    OPT_LEVELS[$i]="${OPT_LEVELS[$i]// /}"
done

if [ ! -d "$HASHES_DIR" ]; then
    echo "Error: Hashes directory not found: $HASHES_DIR"
    exit 1
fi

VALID_OPT_LEVELS="0 1 2 3 s z"
for opt in "${OPT_LEVELS[@]}"; do
    if [[ ! " $VALID_OPT_LEVELS " =~ " $opt " ]]; then
        echo "Error: Invalid optimization level \`$opt\`. Valid levels are: $VALID_OPT_LEVELS"
        exit 1
    fi
done

echo "=== Verifying Reproducible Builds ==="

# Get the names of all platforms provided.
PLATFORMS=""
for dir in "${HASHES_DIR}/${PLATFORM_PREFIX}"*/; do
    # Strip the prefix to get the platform name.
    # Example: "hashes-linux-musl" --> "linux-musl"
    platform=$(basename "$dir" | sed "s/^${PLATFORM_PREFIX}//")
    PLATFORMS="$PLATFORMS $platform"
done
echo "Platforms:$PLATFORMS"

TOTAL_MISMATCHES=0
ALL_MISMATCH_DETAILS=""

# Compare hashes for each optimization level.
for opt in "${OPT_LEVELS[@]}"; do
    echo ""
    echo "=== Processing O${opt} optimization level ==="

    # Use the first platform as the reference for comparison.
    REF_PLATFORM=$(echo $PLATFORMS | awk '{print $1}')
    REF_FILE="${HASHES_DIR}/${PLATFORM_PREFIX}${REF_PLATFORM}/O${opt}.txt"

    if [ ! -f "$REF_FILE" ]; then
        echo "⚠️ WARNING: Reference file not found: $REF_FILE"
        continue
    fi

    REF_COUNT=$(($(wc -l < "$REF_FILE")))
    echo "Reference platform: $REF_PLATFORM ($REF_COUNT hashes)"

    OPT_LEVEL_MISMATCHES=0

    # Compare each platform against the reference.
    for platform in $PLATFORMS; do
        if [ "$platform" = "$REF_PLATFORM" ]; then
            continue
        fi

        OTHER_FILE="${HASHES_DIR}/${PLATFORM_PREFIX}${platform}/O${opt}.txt"

        if [ ! -f "$OTHER_FILE" ]; then
            echo "⚠️ WARNING: File not found: $OTHER_FILE"
            continue
        fi

        OTHER_COUNT=$(($(wc -l < "$OTHER_FILE")))

        # Compare sorted hash files.
        DIFF_OUTPUT=$(diff <(grep ':' "$REF_FILE" | sort) <(grep ':' "$OTHER_FILE" | sort) 2>/dev/null || true)

        if [ -z "$DIFF_OUTPUT" ]; then
            # No differences found.
            echo "$platform: ✅ All $OTHER_COUNT hashes match"
        else
            # Found differences.
            # Get the number of mismatches by extracting the contract IDs (`path:ContractName`, not including
            # the hash) from the diff output, deduplicating it, and counting the unique contracts, because:
            #   - A contract with different hashes appears twice (< and >) with different hashes.
            #   - A missing contract appears once (< or >).
            MISMATCH_COUNT=$(($(echo "$DIFF_OUTPUT" | grep '^[<>]' | sed 's/^[<>] //' | cut -d':' -f1,2 | sort -u | wc -l)))
            echo "$platform: ❌ $MISMATCH_COUNT contracts have different hashes"

            OPT_LEVEL_MISMATCHES=$((OPT_LEVEL_MISMATCHES + MISMATCH_COUNT))

            # Collect details of the first 10 mismatched contracts.
            # Example output:
            #   Optimization level: Oz
            #       linux-musl vs macos-universal: 2 mismatches
            #       Mismatched contracts (first 10):
            #       - contract: path/to/my_contract.sol:MyContract1
            #         macos-universal: abcd1234
            #         linux-musl: aaaa1111
            #       - contract: path/to/my_contract.sol:MyContract2
            #         macos-universal: efgh5678
            #         linux-musl: MISSING
            CURRENT_DETAILS=$(printf "%s\n%s\n%s\n%s" \
                "Optimization level: O${opt}" \
                "    ${REF_PLATFORM} vs ${platform}: ${MISMATCH_COUNT} mismatches" \
                "    Mismatched contracts (first 10):" \
                "$(echo "$DIFF_OUTPUT" | grep '^<' | head -10 | while read line; do
                CONTRACT=$(echo "$line" | sed 's/^< //' | cut -d':' -f1,2)
                REF_HASH=$(echo "$line" | sed 's/^< //' | cut -d':' -f3)
                OTHER_HASH=$(grep "^${CONTRACT}:" "$OTHER_FILE" | cut -d':' -f3)
                echo "    - contract: $CONTRACT"
                echo "      $REF_PLATFORM: $REF_HASH"
                echo "      $platform: ${OTHER_HASH:-MISSING}"
                done)")
            ALL_MISMATCH_DETAILS="${ALL_MISMATCH_DETAILS}\n\n${CURRENT_DETAILS}"
        fi
    done

    TOTAL_MISMATCHES=$((TOTAL_MISMATCHES + OPT_LEVEL_MISMATCHES))

    if [ "$OPT_LEVEL_MISMATCHES" -eq 0 ]; then
        echo "✅ All platforms match for optimization level O${opt}"
    fi
done

# Show final summary.
echo ""
echo "==========================================="
echo "SUMMARY"
echo "==========================================="
echo ""
if [ "$TOTAL_MISMATCHES" -gt 0 ]; then
    echo "❌ FAILURE: Reproducible build verification failed!"
    echo ""
    echo "Total mismatches: $TOTAL_MISMATCHES"
    echo ""
    echo -e "Details:$ALL_MISMATCH_DETAILS"
    echo ""
    echo "==========================================="
    exit 1
else
    echo "✅ SUCCESS: All platform builds are reproducible!"
    echo ""
    echo "==========================================="
fi
