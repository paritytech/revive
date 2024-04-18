install:
	cargo install --path crates/solidity && \
	npm install && npm fund

test:
	cargo install --path crates/solidity && \
	npm install && npm fund && \
	cargo test --manifest-path crates/solidity/Cargo.toml 
	npm run test:cli

test-solidity:
	cargo test --manifest-path crates/solidity/Cargo.toml

test-cli:
	npm run test:cli

clean:
	cargo clean && \
	rm -rf node_modules && \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts && \
	rm -f ~/.cargo/bin/zksolc && \
	rm -f package-lock.json


