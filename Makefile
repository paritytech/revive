.PHONY: install test test-solidity test-cli test-integration clean

install: install-bin install-npm

install-bin:
	cargo install --path crates/solidity

install-npm:
	npm install && npm fund

test: install test-integration test-cli test-solidity

test-integration: install-bin
	cargo test --package revive-integration

test-solidity: install
	cargo test --package revive-solidity

test-cli: install
	npm run test:cli

bench-prepare: install-bin
	cargo criterion --bench prepare --features bench-evm,bench-pvm --message-format=json \
	| criterion-table > crates/benchmarks/PREPARE.md

bench-execute: install-bin
	cargo criterion --bench execute --features bench-evm,bench-pvm --message-format=json \
	| criterion-table > crates/benchmarks/EXECUTE.md

bench: bench-prepare bench-execute

clean:
	cargo clean ; \
	rm -rf node_modules ; \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts ; \
	cargo uninstall revive-solidity ; \
	rm -f package-lock.json
