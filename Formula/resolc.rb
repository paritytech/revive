class Resolc < Formula
desc "Revive Solidity Compiler"
homepage "https://github.com/paritytech/revive"
url "https://github.com/paritytech/revive/releases/download/v0.1.0-dev.14/resolc-universal-apple-darwin"
sha256 "7d5b3cd4233e60e8bfaade398062f1b609166dcb251424cd9d402e51126d8b4f"
license "MIT/Apache-2.0"
  
# If someone wants to build from latest source:
head "https://github.com/paritytech/revive.git", branch: "master"
depends_on "llvm@18" => :build if build.head?    # only needed when building from source
  
    def install
      if build.head?
        # ensure they have a working rust toolchain via rustup
        odie "rustup is required to build Resolc from source; please install it from https://rustup.rs/" \
          unless Utils.which("rustup")
  
        # use rustup's stable toolchain to build in release mode
        system "rustup", "run", "stable", "cargo", "install",
               "--locked",       # lock Cargo.lock for reproducible builds
               "--root", prefix, # install into Homebrew prefix
               "--path", "."     # current repo
      else
        # just strip any quarantine attribute and install the universal binary
        system "xattr", "-c", "resolc-universal-apple-darwin"
        bin.install "resolc-universal-apple-darwin" => "resolc"
      end
    end
  
    test do
      assert_match version.to_s, shell_output("#{bin}/resolc --version")
    end
  end