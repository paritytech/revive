#!/usr/bin/env bash

# Asserts that the compiled project contains the expected compiler output.
# Run from the project's root.

set -euxo pipefail

shopt -s nullglob
build_info_files=(artifacts/build-info/*.json)
if [[ ${#build_info_files[@]} -ne 1 ]]; then
  echo "expected exactly one build-info file under artifacts/build-info/, found ${#build_info_files[@]}" >&2
  exit 1
fi

build_info=${build_info_files[0]}
contract='.output.contracts["contracts/MyToken.sol"].MyToken'
source='.output.sources["contracts/MyToken.sol"]'

jq -er "$contract.evm.bytecode.object"         "$build_info" | grep -q '^50564d'
jq -er "$contract.evm.deployedBytecode.object" "$build_info" | grep -q '^50564d'
jq -e "$contract.evm.methodIdentifiers         | length > 0" "$build_info" > /dev/null
jq -e "$contract.abi                           | length > 0" "$build_info" > /dev/null
jq -e "$contract.storageLayout.storage         | length > 0" "$build_info" > /dev/null
jq -e "$contract.metadata                      | length > 0" "$build_info" > /dev/null
jq -e "$source.ast                             | length > 0" "$build_info" > /dev/null
jq -e '.output.sources                         | length > 1' "$build_info" > /dev/null

echo "all checks passed"
