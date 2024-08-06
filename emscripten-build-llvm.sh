#!/usr/bin/env bash

set -euo pipefail

INSTALL_DIR="${PWD}/llvm18.0-emscripten"
mkdir -p ${INSTALL_DIR}


# Clone LLVM 18 (any revision after commit bd32aaa is supposed to work)
if [ ! -d "llvm-project" ]; then
  git clone --depth 1 --branch release/18.x https://github.com/llvm/llvm-project.git
fi

# Build LLVM, clang
cd llvm-project

# Check if EMSDK_ROOT is defined
if [ -z "$EMSDK_ROOT" ]; then
    echo "Error: EMSDK_ROOT is not defined."
    echo "Please set the EMSDK_ROOT environment variable to the root directory of your Emscripten SDK."
    exit 1
fi

source ${EMSDK_ROOT}/emsdk_env.sh

LLVM_SRC=$(pwd)
LLVM_SRC=$(realpath "$LLVM_SRC")
LLVM_NATIVE=$LLVM_SRC/build-native
LLVM_WASM=$LLVM_SRC/build-wasm

# Cross compiling llvm needs a native build of "llvm-tblgen" and "clang-tblgen"
if [ ! -d $LLVM_NATIVE/ ]; then
    cmake -G Ninja \
        -S $LLVM_SRC/llvm/ \
        -B $LLVM_NATIVE/ \
        -DCMAKE_BUILD_TYPE=Release \
        -DLLVM_TARGETS_TO_BUILD=WebAssembly \
        -DLLVM_ENABLE_PROJECTS="clang"
fi
cmake --build $LLVM_NATIVE/ -- llvm-tblgen clang-tblgen llvm-config

if [ ! -d $LLVM_WASM/ ]; then
	EMCC_DEBUG=2 \
    CXXFLAGS="-Dwait4=__syscall_wait4" \
	LDFLAGS="-lnodefs.js -s NO_INVOKE_RUN -s EXIT_RUNTIME -s INITIAL_MEMORY=64MB -s ALLOW_MEMORY_GROWTH -s EXPORTED_RUNTIME_METHODS=FS,callMain,NODEFS -s MODULARIZE -s EXPORT_ES6 -s WASM_BIGINT" \
	emcmake cmake -G Ninja \
        -S $LLVM_SRC/llvm/ \
        -B $LLVM_WASM/ \
        -DCMAKE_BUILD_TYPE=Release \
        -DLLVM_TARGETS_TO_BUILD='RISCV' \
        -DLLVM_ENABLE_PROJECTS="clang;lld" \
        -DLLVM_ENABLE_DUMP=OFF \
        -DLLVM_ENABLE_ASSERTIONS=OFF \
        -DLLVM_ENABLE_EXPENSIVE_CHECKS=OFF \
        -DLLVM_ENABLE_BACKTRACES=OFF \
        -DLLVM_BUILD_TOOLS=OFF \
        -DLLVM_ENABLE_THREADS=OFF \
        -DLLVM_BUILD_LLVM_DYLIB=OFF \
        -DLLVM_INCLUDE_TESTS=OFF \
		-DLLVM_ENABLE_TERMINFO=Off \
  		-DLLVM_ENABLE_LIBXML2=Off \
  		-DLLVM_ENABLE_ZLIB=Off \
		-DLLVM_ENABLE_ZSTD=Off \
        -DLLVM_TABLEGEN=$LLVM_NATIVE/bin/llvm-tblgen \
        -DCLANG_TABLEGEN=$LLVM_NATIVE/bin/clang-tblgen \
		-DCMAKE_INSTALL_PREFIX=${INSTALL_DIR}/
fi

cmake --build $LLVM_WASM/
cmake --install $LLVM_WASM/

cp $LLVM_NATIVE/bin/llvm-config $INSTALL_DIR/bin

echo ""
echo "success"
