# Codebase organization

## Crates organization

`revive` is organized as a Rust workspace code repository.

### The `crates/` dir

All rust-crates live under the `crates/` directory. The workspace automatically consideres any crate found in there. If you need to add a new create, please it there.

Compiler library crates should be named with the `revive-` prefix. The crate location doesn't need the prefix.

### Dependencies

Dependencies should be added as workspace dependencies. Try to avoid pinning dependencies whenever possible. If possible to do so, add dev dependencies as `dev-dependencies` only.

Please do always include the `Cargo.lock` dependency lock file with your PR. Please don't run `cargo update` together with other changes (it is preferred to update the lock file in a dedicated dependency update PR).
