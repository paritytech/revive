.PHONY: \
	install \
	install-bin \
	install-npm \
	install-wasm \
	install-llvm-builder \
	install-llvm \
	install-revive-runner \
	install-revive-explorer \
	format \
	clippy \
	doc \
	machete \
	test \
	test-integration \
	test-resolc \
	test-yul \
	test-workspace \
	test-wasm \
	test-llvm-builder
	bench \
	bench-pvm \
	bench-evm \
	bench-resolc \
	bench-yul \
	bench-parse-yul \
	bench-lower-yul \
	clean

install: install-bin install-npm

install-bin:
	cargo install --force --locked --path crates/resolc

install-npm:
	npm install && npm fund

install-wasm: install-npm
	cargo build --target wasm32-unknown-emscripten -p resolc --release --no-default-features
	npm run build:package

install-llvm-builder:
	cargo install --force --locked --path crates/llvm-builder

install-llvm: install-llvm-builder
	git submodule update --init --recursive --depth 1
	revive-llvm build --llvm-projects lld --llvm-projects clang

install-revive-runner:
	cargo install --locked --force --path crates/runner --no-default-features

install-revive-explorer:
	cargo install --locked --force --path crates/explorer --no-default-features

format:
	cargo fmt --all --check

clippy:
	cargo clippy --all-features --workspace --tests --benches -- --deny warnings

doc:
	cargo doc --all-features --workspace --document-private-items --no-deps

machete:
	cargo install cargo-machete
	cargo machete

test: format clippy machete test-workspace install-revive-runner install-revive-explorer doc

test-integration: install-bin
	cargo test --package revive-integration

test-resolc: install
	cargo test --package resolc --all-targets

test-yul:
	cargo test --package revive-yul --all-targets

test-workspace: install
	cargo test --workspace --all-targets --exclude revive-llvm-builder

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

bench-resolc: test-resolc
	cargo criterion --package resolc --bench compile --message-format=json \
	| criterion-table > crates/resolc/BENCHMARKS_M4PRO.md

bench-yul: test-yul
	cargo criterion --package revive-yul --bench parse --message-format=json \
	| criterion-table > crates/yul/BENCHMARKS_PARSE_M4PRO.md
	cargo criterion --package revive-yul --bench lower --message-format=json \
	| criterion-table > crates/yul/BENCHMARKS_LOWER_M4PRO.md

clean:
	cargo clean ; \
	revive-llvm clean ; \
	rm -rf node_modules ; \
	rm -rf crates/resolc/src/tests/cli/artifacts ; \
	cargo uninstall resolc ; \
	cargo uninstall revive-llvm-builder ;
