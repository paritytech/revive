// SPDX-License-Identifier: Apache-2.0

use std::{io::Read, process::Command};

fn main() {
    let mut flags = String::new();
    Command::new("llvm-config")
        .args(["--cxxflags"])
        .output()
        .expect("llvm-config should be able to provide CXX flags")
        .stdout
        .as_slice()
        .read_to_string(&mut flags)
        .expect("llvm-config output should be utf8");

    let mut builder = cc::Build::new();
    flags
        .split_whitespace()
        .fold(&mut builder, |builder, flag| builder.flag(flag))
        .cpp(true)
        .file("src/linker.cpp")
        .compile("liblinker.a");

    println!("cargo:rerun-if-changed=build.rs");
}
