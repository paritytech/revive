fn main() {
    let git_rev = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|out| String::from_utf8(out.stdout).unwrap_or_default())
        .unwrap_or("unknown".to_owned());

    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_rev.trim());

    println!("cargo:rustc-link-search=/usr/lib/clang/17/lib/linux");
    println!("cargo:rust-link-lib=static=clang_rt.builtins-aarch64");
}
