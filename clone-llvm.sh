#!/bin/bash
# Default directory for cloning the llvm-project repository
DEFAULT_DIR="llvm-project"

# Check if a directory argument is provided
if [ $# -eq 1 ]; then
  DIR=$1
else
  DIR=$DEFAULT_DIR
fi

# Clone LLVM 18 (any revision after commit bd32aaa is supposed to work)
if [ ! -d "${DIR}" ]; then
  git clone --depth 1 --branch release/18.x https://github.com/llvm/llvm-project.git "${DIR}"
fi
