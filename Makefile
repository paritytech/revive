.PHONY: install test test-solidity test-cli test-integration clean

install:
	cargo install --path crates/solidity && \
	npm install && npm fund

test: install test-integration test-cli test-solidity

test-integration: install
	cargo test --package revive-integration

test-solidity: install
	cargo test --package revive-solidity

test-cli: install
	npm run test:cli

clean:
	cargo clean && \
	rm -rf node_modules && \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts && \
	cargo uninstall revive-solidity && \
	rm -f package-lock.json