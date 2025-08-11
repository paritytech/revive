# revive LLVM builder

Parity fork of the [Matter Labs zksync LLVM builder](https://github.com/matter-labs/era-compiler-llvm-builder) helper utility for compiling [revive](https://github.com/paritytech/revive) compatible LLVM builds.

## Installation and usage

The LLVM compiler framework for revive must be built with our tool called `revive-llvm`.
This is because the revive compiler has requirements not fullfilled in upstream builds:
- Special builds for compiling the frontend into statically linked ELF binaries and also Wasm executables
- The RISC-V target (the PolkaVM target)
- The compiler-rt builtins for the PolkaVM target
- We want to leave the assertions always on
- Various other specific configurations and optimization may be applied

Obtain a compatible build for your host platform from the release section of this repository (TODO). Alternatively follow below steps to get a custom build:

<details>
<summary>1. Install the system prerequisites.</summary>

   * Linux (Debian):

      Install the following packages:
      ```shell
      apt install cmake ninja-build curl git libssl-dev pkg-config clang lld
      ```
   * Linux (Arch):

      Install the following packages:
      ```shell
      pacman -Syu which cmake ninja curl git pkg-config clang lld
      ```

   * MacOS:

      * Install the [HomeBrew](https://brew.sh) package manager.
      * Install the following packages:

         ```shell
         brew install cmake ninja coreutils
         ```

      * Install your choice of a recent LLVM/[Clang](https://clang.llvm.org) compiler, e.g. via [Xcode](https://developer.apple.com/xcode/), [Apple’s Command Line Tools](https://developer.apple.com/library/archive/technotes/tn2339/_index.html), or your preferred package manager.
</details>

<details>
<summary>2. Install Rust.</summary>

   * Follow the latest [official instructions](https://www.rust-lang.org/tools/install:
      ```shell
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
      . ${HOME}/.cargo/env
      ```

      > Currently we are not pinned to any specific version of Rust, so just install the latest stable build for your   platform.
</details>

<details>
<summary>3. Install the revive LLVM framework builder.</summary>

   * Install the builder using `cargo`:
      ```shell
      cargo install --force --locked --path crates/llvm-builder
      ```

      > The builder is not the LLVM framework itself, but a tool that clones its repository and runs a sequence of build commands. By default it is installed in `~/.cargo/bin/`, which is recommended to be added to your `$PATH`.

</details>

<details>
<summary>4. (Optional) Create the `LLVM.lock` file.</summary>

   * The `LLVM.lock` dictates the LLVM source tree being used.
     A default `./LLVM.lock` pointing to the release used for development is already provided.

</details>

<details>
<summary>5. Build LLVM.</summary>

   * Clone and build the LLVM framework using the `revive-llvm` tool.

     The clang and lld projects are required for the `resolc` Solidity frontend executable; they are enabled by default. LLVM assertions are also enabled by default.

      ```shell
      revive-llvm clone
      revive-llvm build --llvm-projects lld --llvm-projects clang
      ```

      Build artifacts end up in the `./target-llvm/gnu/target-final/` directory by default.
      The `gnu` directory depends on the supported archticture and will either be `gnu`, `musl` or `emscripten`.
      You now need to export the final target directory `$LLVM_SYS_181_PREFIX`:

      ```shell
      export LLVM_SYS_181_PREFIX=${PWD}/target-llvm/gnu/target-final
      ```

      If built with the `--enable-tests` option, test tools will be in the `./target-llvm/gnu/build-final/` directory, along with copies of the build artifacts. For all supported build options, run `revive-llvm build --help`.

</details>

## Supported target architectures

The following target platforms are supported:
- Linux GNU (x86)
- Linux MUSL (x86)
- MacOS (aarch64)
- Windows GNU (x86)
- Emscripten (wasm32)

<details>
<summary>Building for MUSL</summary>

   * Via a musl build we can build revive into fully static ELF binaries.
     Which is desirable for reproducible Solidity contracts builds.
     The resulting binary is also very portable, akin to the`solc` frontend binary distribution.

     Clone and build the LLVM framework using the `revive-llvm` tool:
      ```shell
      revive-llvm --target-env musl clone
      revive-llvm --target-env musl build --enable-assertions --llvm-projects clang --llvm-projects lld 
      ```

</details>

<details>
<summary>Building for Emscripten</summary>

   * Via an emsdk build we can run revive in the browser and on node.js.

     Clone and build the LLVM framework using the `revive-llvm` tool:
      ```shell
      revive-llvm --target-env emscripten clone
      revive-llvm --target-env emscripten build --enable-assertions --llvm-projects clang --llvm-projects lld 
      ```

</details>

