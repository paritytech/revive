#!/bin/bash

# This script updates `index.md` in the GitHub Pages root directory provided
# by the required argument to be passed. The file will be updated to simply
# render the `resolc-bin` JSON data for each of the supported platforms.
# `index.md` is the file served by GitHub Pages after being built by Jekyll
# and the Markdown processed by kramdown.

if [ $# -eq 0 ]; then
  echo "Error: The path to the GitHub Pages root directory must be passed"
  exit 1
fi

gh_pages_root_dir="$1"

linux="$gh_pages_root_dir/linux/list.json"
macos="$gh_pages_root_dir/macos/list.json"
nightly_linux="$gh_pages_root_dir/nightly/linux/list.json"
nightly_macos="$gh_pages_root_dir/nightly/macos/list.json"
nightly_wasm="$gh_pages_root_dir/nightly/wasm/list.json"
nightly_windows="$gh_pages_root_dir/nightly/windows/list.json"
wasm="$gh_pages_root_dir/wasm/list.json"
windows="$gh_pages_root_dir/windows/list.json"

build_info_files=("$linux" "$macos" "$nightly_linux" "$nightly_macos"
                  "$nightly_wasm" "$nightly_windows" "$wasm" "$windows")

for file in "${build_info_files[@]}"; do
    if [ ! -f "$file" ]; then
        echo "Error: File does not exist - $file"
        exit 1
    fi
done

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

{% highlight json %}
$(cat $linux)
{% endhighlight %}

</details>

## MacOS

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $macos)
{% endhighlight %}

</details>

## Nightly

### Linux

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $nightly_linux)
{% endhighlight %}

</details>

### MacOS

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $nightly_macos)
{% endhighlight %}

</details>

### Wasm

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $nightly_wasm)
{% endhighlight %}

</details>

### Windows

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $nightly_windows)
{% endhighlight %}

</details>

## Wasm

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $wasm)
{% endhighlight %}

</details>

## Windows

<details>
    <summary>See builds</summary>

{% highlight json %}
$(cat $windows)
{% endhighlight %}

</details>
EOF

echo "File has been updated!"
