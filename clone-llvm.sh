#!/bin/bash

# Clone LLVM 18 (any revision after commit bd32aaa is supposed to work)
if [ ! -d "llvm-project" ]; then
  git clone --depth 1 --branch release/18.x https://github.com/llvm/llvm-project.git
fi
