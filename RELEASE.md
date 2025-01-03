# Release checklist

Prior to the first stable release we neither have formal release processes nor do we follow a fixed release schedule.

To create a new pre-release:

1. Merge a release PR which updates the `-dev.X` versions in the workspace `Cargo.toml` and updates the `CHANGELOG.md` accordingly
2. Push a release tag to `main`
3. Manually trigger the `Build revive-debian` action
4. Create a __pre-release__ from the tag and manually upload the build artifact generated by the action
5. Manually upload `resolc.js` and `resolc.wasm` from the `build-revive-wasm` action artifacts.
6. Update the [contract-docs](https://github.com/paritytech/contract-docs/) accordingly
