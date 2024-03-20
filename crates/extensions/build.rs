use std::{env, fs, path::Path, process::Command};

fn compile(source_path: &str, output_path: &str) {
    let output = Command::new("llc")
        .args([
            "-O3",
            "-filetype=asm",
            "-mattr=+zbb,+e",
            source_path,
            "-o",
            output_path,
        ])
        .output()
        .expect("should be able to invoke llc");

    assert!(
        output.status.success(),
        "failed to compile {}: {:?}",
        source_path,
        output
    );
}

fn main() {
    let in_file = "bswap.ll";
    let out_file = "bswap.s";
    let out_dir = env::var_os("OUT_DIR").expect("env should have $OUT_DIR");
    let out_path = Path::new(&out_dir).join(out_file);
    compile(
        in_file,
        out_path.to_str().expect("$OUT_DIR should be UTF-8"),
    );

    let src_path = Path::new(&out_dir).join("bswap.rs");
    let src = format!("pub static ASSEMBLY: &str = include_str!(\"{out_file}\");");
    fs::write(src_path, src).expect("should be able to write in $OUT_DIR");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=bswap.ll");
}
