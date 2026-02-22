# Cross compilation

We cross-compile the `resolc.js` frontend executable to Wasm for running it in a Node.js or browser environment.

The [musl](https://www.musl-libc.org/) target is used to obtain statically linked ELF binaries for Linux.

## Wasm via emscripten

The `REVIVE_LLVM_TARGET_PREFIX` environment variable is used to control the target environment LLVM dependency. This requires a compatible LLVM build, obtainable via the `revive-llvm` build script. Example:

```sh
# Build the host LLVM dependency with PolkaVM target support
make install-llvm
export LLVM_SYS_211_PREFIX=${PWD}/target-llvm/gnu/target-final

# Build the target LLVM dependency with PolkaVM target support
revive-llvm emsdk
source emsdk/emsdk_env.sh
revive-llvm --target-env emscripten build --llvm-projects lld
export REVIVE_LLVM_TARGET_PREFIX=${PWD}/target-llvm/emscripten/target-final

# Build the resolc frontend executable
make install-wasm
make test-wasm
```

## musl libc

[rust-musl-cross](https://github.com/rust-cross/rust-musl-cross) is a straightforward way to cross compile Rust to musl. The [Dockerfile](https://github.com/paritytech/revive/blob/main/Dockerfile) is an executable example of how to do that. 

