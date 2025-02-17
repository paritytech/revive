# Release checklist

Prior to the first stable release we neither have formal release processes nor do we follow a fixed release schedule.

To create a new pre-release:

1. Merge a release PR which updates the `-dev.X` versions in the workspace `Cargo.toml` and updates the `CHANGELOG.md` accordingly
2. Wait for the `Release` workflow to finish. If the workflow fails after the `build-linux-all` step, check if a tag has been created and delete it before restarting or pushing updates.
3. Check draft release on [Releases page](https://github.com/paritytech/revive/releases) and publish (should contain `resolc.js`, `resolc.wasm`, `resolc-web.js`, and `resolc-static-linux` release assets)
4. Update the [contract-docs](https://github.com/paritytech/contract-docs/) accordingly