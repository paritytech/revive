# Third-Party Source Patches

Patches in this directory are applied on top of the corresponding third-party source files (e.g. the LLVM submodule) at build time. No manual step should be required, the build tooling applies them automatically.

## Layout and naming

* One subdirectory per patched project.
  * LLVM patches live in `patches/llvm/`.
* Cherry-picks of upstream commits or pull requests are named accordingly.
  * Example: `llvm-pr-190587.patch`
    * `190587` would be the upstream PR number

## Format and content

* The `.patch` files must be `git apply`-compatible unified diffs, with paths relative to the submodule root (e.g. `a/llvm/lib/...`).
* The `.patch` files are forced to LF line endings via `.gitattributes`.

## Behavior

* Patches are applied in lexicographic order.
* Already-applied patches are detected and skipped, so re-running the build is safe.

## Lifecycle

Vendor a patch when a needed fix is not yet in the pinned revision, and delete the patch file when the pinned revision is bumped to one that contains the fix.
