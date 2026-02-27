# Release checklist

Prior to the first stable release we neither have formal release processes nor do we follow a fixed release schedule.

To create a new pre-release:

1. Create a release PR which, if necessary:
  - Updates the versions in the workspace `Cargo.toml`
  - Updates the version in each crate `Cargo.toml`
  - Updates the version of the NPM package in `js/resolc/package.json`
  - Updates the `CHANGELOG.md` to reflect all observable changes
2. If the CI passes, merge the release PR.
3. Push a `vX.Y.Z` tag that has the same version as in `Cargo.toml`
4. The release workflow will attempt to build and publish a new pre-release if the latest tag does match the cargo package version.
5. Wait for the `Release` workflow to finish. It should create the pre-release with the same name.
6. Check that pre-release was created on the [Releases page](https://github.com/paritytech/revive/releases) with all artifacts.
7. After the release is published, another workflow should start automatically and update JSON files in the [resolc-bin repository](https://github.com/paritytech/resolc-bin) as well as render the JSON on its [GitHub Pages website](https://paritytech.github.io/resolc-bin/). Check the changes.
8. Update the [revive compiler book](https://github.com/paritytech/revive/tree/main/book) accordingly.

# `resolc` NPM package release

Will happen automatically.

# LLVM release

To create a new LLVM release, run "Release LLVM" workflow. Use current LLVM version as parameter, e.g. `18.1.8`.
Version suffix will be resolved automatically.  
The workflows will create new GitHub release, and upload LLVM binaries.
Next release of resolc will use newly created binaries.  
