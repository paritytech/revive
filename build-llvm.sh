#!/usr/bin/env bash

set -euo pipefail

INSTALL_DIR="${PWD}/llvm18.0"
mkdir -p $INSTALL_DIR


# Clone LLVM 18 (any revision after commit bd32aaa is supposed to work)
if [ ! -d "llvm-project" ]; then
  git clone --depth 1 --branch release/18.x https://github.com/llvm/llvm-project.git
fi


# Build LLVM, clang
cd llvm-project

mkdir -p build
cd build
cmake -G Ninja -DLLVM_ENABLE_ASSERTIONS=On \
  -DLLVM_ENABLE_TERMINFO=Off \
  -DLLVM_ENABLE_LIBXML2=Off \
  -DLLVM_ENABLE_ZLIB=Off \
  -DLLVM_ENABLE_PROJECTS='clang;lld' \
  -DLLVM_TARGETS_TO_BUILD='RISCV' \
  -DLLVM_ENABLE_ZSTD=Off \
  -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_INSTALL_PREFIX=$INSTALL_DIR \
	../llvm

ninja
ninja install


# Build compiler builtins
cd ../compiler-rt
mkdir -p build
cd build

CFLAGS="--target=riscv32 -march=rv32em -mabi=ilp32e -nostdlib -nodefaultlibs -mcpu=generic-rv32"
cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$INSTALL_DIR \
  -DCOMPILER_RT_BUILD_BUILTINS=ON \
  -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
  -DCOMPILER_RT_BUILD_MEMPROF=OFF \
  -DCOMPILER_RT_BUILD_PROFILE=OFF \
  -DCOMPILER_RT_BUILD_SANITIZERS=OFF \
  -DCOMPILER_RT_BUILD_XRAY=OFF \
  -DCMAKE_C_COMPILER=$INSTALL_DIR/bin/clang \
  -DCMAKE_C_COMPILER_TARGET="riscv32" \
  -DCMAKE_ASM_COMPILER_TARGET="riscv32" \
  -DCMAKE_AR=$INSTALL_DIR/bin/llvm-ar \
  -DCMAKE_NM=$INSTALL_DIR/bin/llvm-nm \
  -DCMAKE_RANLIB=$INSTALL_DIR/bin/llvm-ranlib \
  -DCOMPILER_RT_BAREMETAL_BUILD=ON \
  -DLLVM_CONFIG_PATH=$INSTALL_DIR/bin/llvm-config \
  -DCMAKE_C_FLAGS="$CFLAGS" \
  -DCMAKE_ASM_FLAGS="$CFLAGS" \
  -DCOMPILER_RT_TEST_COMPILER=$INSTALL_DIR/bin/clang \
  -DCMAKE_CXX_FLAGS="$CFLAGS" \
  -DCOMPILER_RT_DEFAULT_TARGET_ONLY=ON \
  -DCMAKE_SYSTEM_NAME=Linux \
  ..

ninja
ninja install


echo ""
echo "success"
echo "add this directory to your PATH: ${INSTALL_DIR}/bin/"
