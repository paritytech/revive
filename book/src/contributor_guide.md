# Contributor guide

The `revive` compiler is an open source software project and we gladly accept contributions from anyone!

## Development

For the most parts, `revive` is a rather standard Rust workspace codebase. There are some non-Rust dependencies, which sometimes complicates things a little bit.

### Building the compiler

A quick reference on how to build the Solidity compiler is maintained in the projects [README.md](https://github.com/paritytech/revive?tab=readme-ov-file#building-from-source).

### Using the `Makefile`

The [Makefile](https://github.com/paritytech/revive/blob/main/Makefile) comprehensively encapsulates all development aspects of this codebase. It is kept concise and readable. Please read and use it! You'll learn for example:

- How to build and install a `resolc` development version.
- How to run tests and benchmarks.
- How to cross-compile `resolc`.

As a general rule-of-thumb: If `make test` runs fine locally, chances for green CI pipelines are good.

## Contribution rules

1. Changes must be submitted via a pull request (PR) to the github upstream repository.
2. Ensure that your branch passes `make test` locally when submitting a pull request.
3. A PR must not be merged until CI fully passes. Exceptions can be made (for example to fix CI issues itself).
4. No force pushes to the `main` branch and open PR branches.
5. Maintainers can request changes or deny contributions to their own discretion.

## AI policy

Contributors may use whatever AI assistance tools they wish to whatever degree they wish in the process of creating their contribution, __given they acknowledge the follwing__:

_Project maintainers may reject any contribution (or portions of it) if the contribution shows signs of problematic involvement of generative AI_.

Judgement of "problematic involvement" lies at the sole discretion of project maintainers. No proof (whether a contribution was in fact AI generated or not) is required. Rationale:

- No one enjoys reading soulless and uncanny LLM slop. Please review and fix any AI slop yourself prior to submitting a PR.
- A Solidity compiler is security sensitive software. Even miniscule mistakes can ultimately lead to loss of funds. AI models are inherently stochastic. They regurarly fail to capture important nuances or produce straight hallucinations. Code that was "blindly" generated has no home here.
- `revive` is a large codebase. Generative AI assistants may not have enough "context window" to sufficiently capture correctness, consistency and style aspects of the codebase. We'd like to keep this codebase maintainable _by humans_ for the forseeable future.
