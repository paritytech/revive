# Contributor guide

The `revive` compiler is an open source software project and we gladly accept quality contributions from anyone!

## Getting started

A quick reference on how to build the Solidity compiler is maintained in the project's [README.md](https://github.com/paritytech/revive?tab=readme-ov-file#building-from-source).

### Using the `Makefile`

The [Makefile](https://github.com/paritytech/revive/blob/main/Makefile) comprehensively encapsulates all development aspects of this codebase. It is kept concise and readable. Please read and use it! You'll learn for example:

- How to build and install a `resolc` development version.
- How to run tests and benchmarks.
- How to cross-compile `resolc`.

As a general rule-of-thumb: If `make test` runs fine locally, chances for green CI pipelines are good.

### Codebase organization

For the most parts, `revive` is a rather standard Rust workspace codebase. There are some non-Rust dependencies, which sometimes complicates things a little bit.

#### The `crates/` dir

All Rust crates live under the `crates/` directory. The workspace automatically considers any crate found therein. If you need to add a new create, please implement it there.

Compiler library crates should be named with the `revive-` prefix. The crate location doesn't need the prefix.

#### Dependencies

Dependencies should be added as workspace dependencies. Try to avoid pinning dependencies whenever possible. If possible, add dev dependencies as `dev-dependencies` only.

Please do always include the `Cargo.lock` dependency lock file with your PR. Please don't run `cargo update` together with other changes (it is preferred to update the lock file in a dedicated dependency update PR).

## Contribution rules

1. Changes must be submitted via a pull request (PR) to the github upstream repository.
2. Ensure that your branch passes `make test` locally when submitting a pull request.
3. A PR must not be merged until CI fully passes. Exceptions can be made (for example to fix CI issues itself).
4. No force pushes to the `main` branch and open PR branches.
5. Maintainers can request changes or deny contributions at their own discretion.

## Style guide

We require the official Rust formatter and clippy linter. In addition to that, please also consider the following best-effort aspects:

- Avoid [magic numbers](https://en.wikipedia.org/wiki/Magic_number_(programming)) and strings. Instead, add them as module constants.
- Avoid abbreviated variable and function names. Always provide meaningful and readable symbols.
- Don't write macros and don't use third party macros for things that can easily be expressed in few lines of code or outlined into functions.
- Avoid import aliasing. Please use the parent or fully qualified path for conflicting symbols.
- Any inline comments must provide additional semantic meaning, explain counter-intuitive behavior or highlight non-obvious design decisions. In other words, try to make the code expressive enough to a degree it doesn't need comments expressing the same thing again in the English language. Delete such comments if your AI assistant generated them.
- Public items must have a meaningful doc comment.
- Provide meaningful panic messages to `.expect()` or just use `.unwrap()`.

## AI policy

Contributors may use whatever AI assistance tools they wish to whatever degree they wish in the process of creating their contribution, __given they acknowledge the following__:

_Project maintainers may reject any contribution (or portions of it) if the contribution shows signs of problematic involvement of generative AI_.

Judgement of "problematic involvement" lies at the sole discretion of project maintainers. No proof (whether a contribution was in fact AI generated or not) is required. Rationale:

- No one enjoys reading soulless and uncanny LLM slop. Please review and fix any AI slop yourself prior to submitting a PR.
- A Solidity compiler is security sensitive software. Even miniscule mistakes can ultimately lead to loss of funds. AI models are inherently stochastic. They regurarly fail to capture important nuances or produce straight hallucinations. Code that was "blindly" generated has no home here.
- `revive` is a large codebase. Generative AI assistants may not have enough "context window" to sufficiently capture correctness, consistency and style aspects of the codebase. We'd like to keep this codebase maintainable _by humans_ for the forseeable future.
