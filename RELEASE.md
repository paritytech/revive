# Release checklist

Prior to the first stable release we neither have formal release processes nor do we follow a fixed release schedule.

To create a new pre-release:

1. Merge a release PR which updates the `-dev.X` versions in the workspace `Cargo.toml` and updates the `CHANGELOG.md` accordingly. The release workflow will attempt to build and publish a new release whenever the latest git tag does not match the cargo package version.
2. Wait for the `Release` workflow to finish. If the workflow fails after the `build-linux-all` step, check if a tag has been created and delete it before restarting or pushing updates. Note: It's more convenient to debug the release workflow in a fork (the fork has to be under the `paritytech` org to access `parity-large` runners).
3. Check draft release on [Releases page](https://github.com/paritytech/revive/releases) and publish (should contain `resolc.js`, `resolc.wasm`, `resolc-web.js`, and `resolc-static-linux` release assets)
4. Update the [contract-docs](https://github.com/paritytech/contract-docs/) accordingly

# LLVM release

To create a new LLVM release, create a git tag (not GitHub release) with `llvm-` prefix, e.g. `llvm-0.0.11`.  
`Release LLVM` action will start automatically. It will create new GitHub release, and upload LLVM binaries.  
Other actions including Release will use these binaries on the next run.
