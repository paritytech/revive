class Resolc < Formula
  desc "Revive Solidity Compiler"
  homepage "https://github.com/paritytech/revive"
  url "https://github.com/paritytech/revive/releases/download/v0.1.0-dev.14/resolc-universal-apple-darwin"
  sha256 "7d5b3cd4233e60e8bfaade398062f1b609166dcb251424cd9d402e51126d8b4f"
  license "MIT/Apache-2.0"

  depends_on "rust" => :build
  depends_on "llvm@18" => :build

  def install
    system "xattr", "-c", "resolc-universal-apple-darwin"
    bin.install "resolc-universal-apple-darwin" => "resolc"
  end

  test do
    system "#{bin}/resolc", "--version"
  end
end 