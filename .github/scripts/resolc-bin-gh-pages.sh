#!/bin/bash

# This script updates `index.md` in the directory provided by the required
# `GH_PAGES_ROOT_DIR` environment variable. The file will be updated to
# render the `resolc-bin` JSON data for each of the supported platforms.
# `index.md` is the file served by GitHub Pages after being built by Jekyll
# and the Markdown processed by kramdown.

if [ -z "$GH_PAGES_ROOT_DIR" ]; then
    echo "Error: GH_PAGES_ROOT_DIR environment variable is not set."
    exit 1
fi

linux="$GH_PAGES_ROOT_DIR/linux/list.json"
macos="$GH_PAGES_ROOT_DIR/macos/list.json"
nightly_linux="$GH_PAGES_ROOT_DIR/nightly/linux/list.json"
nightly_macos="$GH_PAGES_ROOT_DIR/nightly/macos/list.json"
nightly_wasm="$GH_PAGES_ROOT_DIR/nightly/wasm/list.json"
nightly_windows="$GH_PAGES_ROOT_DIR/nightly/windows/list.json"
wasm="$GH_PAGES_ROOT_DIR/wasm/list.json"
windows="$GH_PAGES_ROOT_DIR/windows/list.json"

build_info_files=("$linux" "$macos" "$nightly_linux" "$nightly_macos"
                  "$nightly_wasm" "$nightly_windows" "$wasm" "$windows")

for file in "${build_info_files[@]}"; do
    if [ ! -f "$file" ]; then
        echo "Error: File does not exist - $file"
        exit 1
    fi
done

echo "Updating GitHub Pages index.md file..."

cat > "$GH_PAGES_ROOT_DIR/index.md" << EOF
---
title: resolc-bin
---

# resolc-bin

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
