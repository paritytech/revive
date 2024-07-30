#!/usr/bin/env bash

set -euo pipefail

INSTALL_DIR="${PWD}/llvm18.0"
mkdir -p ${INSTALL_DIR}


# Clone LLVM 18 (any revision after commit bd32aaa is supposed to work)
if [ ! -d "llvm-project" ]; then
  git clone --depth 1 --branch release/18.x https://github.com/llvm/llvm-project.git
fi


# Build LLVM, clang
LLVM_SRC_PREFIX=${PWD}/llvm-project
LLVM_SRC_DIR=${LLVM_SRC_PREFIX}/llvm
LLVM_BUILD_DIR=${PWD}/build/llvm
if [ ! -d ${LLVM_BUILD_DIR} ] ; then
	mkdir -p ${LLVM_BUILD_DIR}
fi

EMSDK_ROOT=/Users/smiasojed/Development/emsdk

source ${EMSDK_ROOT}/emsdk_env.sh

LLVM_SRC=$(pwd)
BUILD=${LLVM_SRC}/build
BUILD=$(realpath "$BUILD")
LLVM_BUILD=$BUILD/llvm-wasm
LLVM_NATIVE=$BUILD/llvm-native

# Cross compiling llvm needs a native build of "llvm-tblgen" and "clang-tblgen"
if [ ! -d $LLVM_NATIVE/ ]; then
    cmake -G Ninja \
        -S $LLVM_SRC/llvm/ \
        -B $LLVM_NATIVE/ \
        -DCMAKE_BUILD_TYPE=Release \
        -DLLVM_TARGETS_TO_BUILD=WebAssembly \
        -DLLVM_ENABLE_PROJECTS="clang" \
		-DCMAKE_INSTALL_PREFIX=${INSTALL_DIR}
fi
cmake --build $LLVM_NATIVE/ -- llvm-tblgen clang-tblgen

if [ ! -d $LLVM_BUILD/ ]; then
	EMCC_DEBUG=2 \
    CXXFLAGS="-Dwait4=__syscall_wait4" \
	LDFLAGS="-s NO_INVOKE_RUN -s EXIT_RUNTIME -s INITIAL_MEMORY=64MB -s ALLOW_MEMORY_GROWTH -s EXPORTED_RUNTIME_METHODS=FS,callMain -s MODULARIZE -s EXPORT_ES6 -s WASM_BIGINT" \
	emcmake cmake -G Ninja \
        -S $LLVM_SRC/llvm/ \
        -B $LLVM_BUILD/ \
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
		-DCMAKE_INSTALL_PREFIX=${INSTALL_DIR}/wasm
fi

cmake --build $LLVM_BUILD/
cmake --install $LLVM_BUILD/

# Build LLVM, clang, RISCV target
LLVM_NATIVE_RISCV_BUILD=$BUILD/llvm-native-riscv
cmake -G Ninja \
  -S $LLVM_SRC/llvm/ \
  -B $LLVM_NATIVE_RISCV_BUILD/ \
  -DLLVM_ENABLE_ASSERTIONS=On \
  -DLLVM_ENABLE_TERMINFO=Off \
  -DLLVM_ENABLE_LIBXML2=Off \
  -DLLVM_ENABLE_ZLIB=Off \
  -DLLVM_ENABLE_PROJECTS='clang;lld' \
  -DLLVM_TARGETS_TO_BUILD='RISCV' \
  -DLLVM_ENABLE_ZSTD=Off \
  -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_INSTALL_PREFIX=${INSTALL_DIR}

cmake --build $LLVM_NATIVE_RISCV_BUILD/
cmake --install $LLVM_NATIVE_RISCV_BUILD/

# Build compiler builtins
COMPILER_RT_BUILD=$BUILD/compiler_rt

build_compiler_rt() {
	case "$1" in
		64) TARGET_ABI=lp64e ;;
		32) TARGET_ABI=ilp32e ;;
		*) exit -1
	esac
	CFLAGS="--target=riscv${1} -march=rv${1}em -mabi=${TARGET_ABI} -mcpu=generic-rv${1} -nostdlib -nodefaultlibs"

	cmake -G Ninja \
	  -S $LLVM_SRC/compiler-rt/ \
      -B $COMPILER_RT_BUILD/ \
	  -DCMAKE_BUILD_TYPE=Release \
	  -DCMAKE_INSTALL_PREFIX=${INSTALL_DIR} \
	  -DCOMPILER_RT_BUILD_BUILTINS=ON \
	  -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
	  -DCOMPILER_RT_BUILD_MEMPROF=OFF \
	  -DCOMPILER_RT_BUILD_PROFILE=OFF \
	  -DCOMPILER_RT_BUILD_SANITIZERS=OFF \
	  -DCOMPILER_RT_BUILD_XRAY=OFF \
	  -DCMAKE_C_COMPILER=${INSTALL_DIR}/bin/clang \
	  -DCMAKE_C_COMPILER_TARGET=riscv${1} \
	  -DCMAKE_ASM_COMPILER_TARGET=riscv${1} \
	  -DCMAKE_CXX_COMPILER_TARGET=riscv${1} \
	  -DCMAKE_C_TARGET_BITS=riscv${1} \
	  -DCMAKE_ASM_TARGET_BITS=riscv${1} \
	  -DCMAKE_AR=${INSTALL_DIR}/bin/llvm-ar \
	  -DCMAKE_NM=${INSTALL_DIR}/bin/llvm-nm \
	  -DCMAKE_RANLIB=${INSTALL_DIR}/bin/llvm-ranlib \
	  -DCOMPILER_RT_BAREMETAL_BUILD=ON \
	  -DLLVM_CONFIG_PATH=${INSTALL_DIR}/bin/llvm-config \
	  -DCMAKE_C_FLAGS="${CFLAGS}" \
	  -DCMAKE_ASM_FLAGS="${CFLAGS}" \
	  -DCOMPILER_RT_TEST_COMPILER=${INSTALL_DIR}/bin/clang \
	  -DCMAKE_CXX_FLAGS="${CFLAGS}" \
	  -DCMAKE_SYSTEM_NAME=unknown \
	  -DCOMPILER_RT_DEFAULT_TARGET_ONLY=ON
	
	cmake --build $COMPILER_RT_BUILD/
	cmake --install $COMPILER_RT_BUILD/
}

build_compiler_rt 32
build_compiler_rt 64

echo ""
echo "success"
echo "add this directory to your PATH: ${INSTALL_DIR}/bin/"
