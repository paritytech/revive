#!/bin/bash

# This script updates `index.md` in the GitHub Pages root directory provided
# by the required argument to be passed. The file will be updated to render
# the `resolc-bin` release data as a formatted table for each supported platform.
# `index.md` is the file served by GitHub Pages after being built by Jekyll
# and the Markdown processed by kramdown.

set -exo pipefail

gh_pages_root_dir="$1"
if [ -z "$gh_pages_root_dir" ]; then
  echo "Error: The path to the GitHub Pages root directory must be passed"
  exit 1
fi

linux="$gh_pages_root_dir/linux/list.json"
macos="$gh_pages_root_dir/macos/list.json"
wasm="$gh_pages_root_dir/wasm/list.json"
windows="$gh_pages_root_dir/windows/list.json"
nightly_linux="$gh_pages_root_dir/nightly/linux/list.json"
nightly_macos="$gh_pages_root_dir/nightly/macos/list.json"
nightly_wasm="$gh_pages_root_dir/nightly/wasm/list.json"
nightly_windows="$gh_pages_root_dir/nightly/windows/list.json"

build_info_files=("$linux" "$macos" "$wasm" "$windows" "$nightly_linux"
                  "$nightly_macos" "$nightly_wasm" "$nightly_windows")

for file in "${build_info_files[@]}"; do
    if [ ! -f "$file" ]; then
        echo "Error: File does not exist - $file"
        exit 1
    fi
done

# Render builds as a markdown table with clickable links
render_builds_table() {
    local build_info_file="$1"
    if [ -z "$build_info_file" ]; then
        echo "Error: A file path argument is required"
        return 1
    fi

    # Sort builds by version descending and render as markdown table
    jq -r '
        .builds | sort_by(.version) | reverse |
        ["| Release | Solc Versions | SHA256 |", "|---------|---------------|--------|"] +
        [.[] |
            "| [\(.name) \(.longVersion)](\(.url)) | \(.firstSolcVersion) - \(.lastSolcVersion) | `\(.sha256[0:16])...` |"
        ] | .[]
    ' "$build_info_file"
}

echo "Updating GitHub Pages index.md file..."

cat > "$gh_pages_root_dir/index.md" << EOF
---
title: resolc-bin
---

# resolc-bin

Listed here are details about the \`resolc\` binary releases for the supported platforms.
The information is synced with the [resolc-bin GitHub repository](https://github.com/paritytech/resolc-bin).

## Linux

<details>
<summary>See builds</summary>

$(render_builds_table $linux)

</details>

## MacOS

<details>
<summary>See builds</summary>

$(render_builds_table $macos)

</details>

## Wasm

<details>
<summary>See builds</summary>

$(render_builds_table $wasm)

</details>

## Windows

<details>
<summary>See builds</summary>

$(render_builds_table $windows)

</details>

## Nightly

### Linux

<details>
<summary>See builds</summary>

$(render_builds_table $nightly_linux)

</details>

### MacOS

<details>
<summary>See builds</summary>

$(render_builds_table $nightly_macos)

</details>

### Wasm

<details>
<summary>See builds</summary>

$(render_builds_table $nightly_wasm)

</details>

### Windows

<details>
<summary>See builds</summary>

$(render_builds_table $nightly_windows)

</details>
EOF

echo "File has been updated!"
