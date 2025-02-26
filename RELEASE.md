# Release checklist

Prior to the first stable release we neither have formal release processes nor do we follow a fixed release schedule.

To create a new pre-release:

1. Create a release PR which updates the `-dev.X` versions in the workspace `Cargo.toml` and updates the `CHANGELOG.md` accordingly. Add the `release-test` label to trigger the release workflows.
2. If the CI passes, merge the release PR. The release workflow will attempt to build and publish a new release whenever the latest git tag does not match the cargo package version.
3. Wait for the `Release` workflow to finish. If the workflow fails after the `build-linux-all` step, check if a tag has been created and delete it before restarting or pushing updates. Note: It's more convenient to debug the release workflow in a fork (the fork has to be under the `paritytech` org to access `parity-large` runners).
4. Check draft release on [Releases page](https://github.com/paritytech/revive/releases) and publish (should contain `resolc.js`, `resolc.wasm`, `resolc-web.js`, and `resolc-static-linux` release assets)
5. Update the [contract-docs](https://github.com/paritytech/contract-docs/) accordingly

# LLVM release

To create a new LLVM release, run "Release LLVM" workflow. Use current LLVM version as parameter, e.g. `18.1.8`.
Version suffix will be resolved automatically.  
The workflows will create new GitHub release, and upload LLVM binaries.
Next release of resolc will use newly created binaries.  
