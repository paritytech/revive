.PHONY: install format test test-solidity test-cli test-integration test-workspace clean docs docs-build

install: install-bin install-npm

install-bin:
	cargo install --path crates/solidity

install-npm:
	npm install && npm fund

format:
	cargo fmt --all --check

test: format clippy test-cli test-workspace
	cargo test --workspace

test-integration: install-bin
	cargo test --package revive-integration

test-solidity: install
	cargo test --package revive-solidity

test-workspace: install
	cargo test --workspace

test-cli: install
	npm run test:cli

bench-pvm: install-bin
	cargo criterion --bench execute --features bench-pvm-interpreter --message-format=json \
	| criterion-table > crates/benchmarks/PVM.md

bench-evm: install-bin
	cargo criterion --bench execute --features bench-evm --message-format=json \
	| criterion-table > crates/benchmarks/EVM.md

bench: install-bin
	cargo criterion --all --all-features --message-format=json \
	| criterion-table > crates/benchmarks/BENCHMARKS.md

clippy:
	cargo clippy --all-features --workspace --tests --benches

docs: docs-build
	mdbook serve --open docs/

docs-build:
	mdbook test docs/ && mdbook build docs/

clean:
	cargo clean ; \
	rm -rf node_modules ; \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts ; \
	cargo uninstall revive-solidity ; \
	rm -f package-lock.json
