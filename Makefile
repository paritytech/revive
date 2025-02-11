.PHONY: \
	install \
	install-bin \
	install-npm \
	install-wasm \
	install-llvm-builder \
	install-llvm \
	format \
	clippy \
	machete \
	test \
	test-integration \
	test-solidity \
	test-workspace \
	test-cli \
	test-wasm \
	test-llvm-builder
	bench \
	bench-pvm \
	bench-evm \
	clean

install: install-bin install-npm

install-bin:
	cargo install --path crates/solidity

install-npm:
	npm install && npm fund

install-wasm: install-npm
	cargo build --target wasm32-unknown-emscripten -p revive-solidity --release --no-default-features
	npm run build:package

install-llvm-builder:
	cargo install --path crates/llvm-builder

install-llvm: install-llvm-builder
	revive-llvm clone
	revive-llvm build --llvm-projects lld --llvm-projects clang

format:
	cargo fmt --all --check

clippy:
	cargo clippy --all-features --workspace --tests --benches -- --deny warnings

machete:
	cargo install cargo-machete
	cargo machete

test: format clippy machete test-cli test-workspace

test-integration: install-bin
	cargo test --package revive-integration

test-solidity: install
	cargo test --package revive-solidity

test-workspace: install
	cargo test --workspace --exclude revive-llvm-builder

test-cli: install
	npm run test:cli

test-wasm: install-wasm
	npm run test:wasm

test-llvm-builder:
	@echo "warning: the llvm-builder tests will take many hours"
	cargo test --package revive-llvm-builder -- --test-threads=1

bench: install-bin
	cargo criterion --all --all-features --message-format=json \
	| criterion-table > crates/benchmarks/BENCHMARKS.md

bench-pvm: install-bin
	cargo criterion --bench execute --features bench-pvm-interpreter --message-format=json \
	| criterion-table > crates/benchmarks/PVM.md

bench-evm: install-bin
	cargo criterion --bench execute --features bench-evm --message-format=json \
	| criterion-table > crates/benchmarks/EVM.md

clean:
	cargo clean ; \
	revive-llvm clean ; \
	rm -rf node_modules ; \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts ; \
	cargo uninstall revive-solidity ; \
	cargo uninstall revive-llvm-builder ; \
	rm -f package-lock.json ; \
	rm -rf js/dist ; \
	rm -f js/src/resolc.{wasm,js}
