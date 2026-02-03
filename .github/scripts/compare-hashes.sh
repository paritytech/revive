#!/bin/bash

# This script compares bytecode hashes across the provided platforms
# to verify that resolc generates identical bytecode. Hashes are
# compared for each optimization level (O0, O3, Oz).
#
# Usage: compare-hashes.sh <hashes-dir> [platform-prefix]
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

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
    echo "Error: Expected 1 or 2 arguments, got $#"
    echo "Usage: $0 <hashes-directory> [platform-prefix]"
    exit 1
fi

HASHES_DIR="${1%/}"
PLATFORM_PREFIX="${2:-}"

if [ ! -d "$HASHES_DIR" ]; then
    echo "Error: Hashes directory not found: $HASHES_DIR"
    exit 1
fi

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
for opt in O0 O3 Oz; do
    echo ""
    echo "=== Processing $opt optimization level ==="

    # Use the first platform as the reference for comparison.
    REF_PLATFORM=$(echo $PLATFORMS | awk '{print $1}')
    REF_FILE="${HASHES_DIR}/${PLATFORM_PREFIX}${REF_PLATFORM}/${opt}.txt"

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

        OTHER_FILE="${HASHES_DIR}/${PLATFORM_PREFIX}${platform}/${opt}.txt"

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
            # Found differences. Count lines starting with < or >.
            MISMATCH_COUNT=$(echo "$DIFF_OUTPUT" | grep -c '^[<>]') || MISMATCH_COUNT=0
            # Divide by 2 as each mismatch shows as both < and >.
            MISMATCH_COUNT=$((MISMATCH_COUNT / 2))
            echo "$platform: ❌ $MISMATCH_COUNT contracts have different hashes"

            ((OPT_LEVEL_MISMATCHES += MISMATCH_COUNT))

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
                "Optimization level: ${opt}" \
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

    ((TOTAL_MISMATCHES += OPT_LEVEL_MISMATCHES))

    if [ "$OPT_LEVEL_MISMATCHES" -eq 0 ]; then
        echo "✅ All platforms match for optimization level $opt"
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
