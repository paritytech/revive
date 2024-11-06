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

$(pwd)/build-llvm.sh
export PATH=$(pwd)/llvm18.0/bin:$PATH

make install-revive REVIVE_INSTALL_DIR=${REVIVE_INSTALL_DIR}
