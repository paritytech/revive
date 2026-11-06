#!/usr/bin/env bash

# Asserts that the compiled project contains the expected compiler output.
# Requires `forge` in PATH. Run from the project's root.
#
# Usage: verify-compiler-output.sh <resolc-path>

set -euxo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $(basename "$0") <resolc-path>" >&2
    exit 2
fi

resolc=$1

inspect() {
    forge inspect --use-resolc "$resolc" MyToken "$@"
}

inspect bytecode          | grep '^0x50564d'                      > /dev/null
inspect deployedBytecode  | grep '^0x50564d'                      > /dev/null
inspect irOptimized       | grep .                                > /dev/null
inspect ir                | grep .                                > /dev/null
inspect assembly          | grep .                                > /dev/null
inspect abi               --json | jq -e 'length > 0'             > /dev/null
inspect methodIdentifiers --json | jq -e 'length > 0'             > /dev/null
inspect storageLayout     --json | jq -e '.storage | length > 0'  > /dev/null
inspect metadata          --json | jq -e 'length > 0'             > /dev/null
inspect devdoc            --json | jq -e 'length > 0'             > /dev/null
inspect userdoc           --json | jq -e 'length > 0'             > /dev/null
# solc only injects this revert string when `debug` is used.
inspect irOptimized       --revert-strings debug | grep 'Ether sent to non-payable' > /dev/null

echo "all checks passed"
