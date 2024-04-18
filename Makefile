build:
	cargo build && \
	cp target/debug/zksolc ~/.cargo/bin/zksolc && \
	npm install && npm fund

build-release:
	cargo build --release && \
	cp target/release/zksolc ~/.cargo/bin/zksolc && \
	npm install && npm fund

clean:
	cargo clean && \
	rm -rf node_modules && \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts && \
	rm -f ~/.cargo/bin/zksolc && \
	rm -f package-lock.json

test:
	npm run test:cli
