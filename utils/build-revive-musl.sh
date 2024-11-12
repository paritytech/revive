#! /usr/bin/env bash

set -euo pipefail

REVIVE_INSTALL_DIR=$(pwd)/target/release
while getopts "o:" option ; do
    case $option in
    o) # Output directory
        REVIVE_INSTALL_DIR=$OPTARG
        ;;
    \?) echo "Error: Invalid option"
        exit 1;;
    esac
done
echo "Installing to ${REVIVE_INSTALL_DIR}"

#export CC=clang
#export CXX=clang++

#$(pwd)/build-llvm.sh
export PATH=$(pwd)/llvm18.0/bin:$PATH

cargo clean

export RUSTFLAGS="-C target-feature=+crt-static"
#export RUSTFLAGS+=" -l static=c"
#export RUSTFLAGS+=" -l static=stdc++"
#export RUSTFLAGS+=" -l static=tinfo"
#export RUSTFLAGS+=" -L/usr/lib"
export RUSTFLAGS+=" -l static=clang_rt.builtins-aarch64"
#export RUSTFLAGS+=" -l static=clang_rt.builtins-riscv32"
export RUSTFLAGS+=" -L/usr/lib/clang/17/lib/linux"
export RUSTFLAGS+=" -L$(pwd)/llvm18.0/lib"
export RUSTFLAGS+=" -L$(pwd)/llvm18.0/lib/unknown"
cargo install --path crates/solidity \
    -vv \
    --root ${REVIVE_INSTALL_DIR} \
    --target=aarch64-unknown-linux-musl


