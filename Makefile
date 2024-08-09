.PHONY: install format test test-solidity test-cli test-integration test-workspace clean docs docs-build

ifeq ($(strip $(WASM_INSTALL_PREFIX)),)
WASM_INSTALL_PREFIX=$(shell pwd)
endif
$(info "WASM_INSTALL_PREFIX=$(WASM_INSTALL_PREFIX))

install: install-bin install-npm

install-bin:
	cargo install --path crates/solidity

install-wasm:
	RUSTFLAGS='-Clink-arg=-sEXPORTED_FUNCTIONS=_main,_free,_malloc -Clink-arg=-sNO_INVOKE_RUN -Clink-arg=-sEXIT_RUNTIME -Clink-arg=-sINITIAL_MEMORY=64MB -Clink-arg=-sALLOW_MEMORY_GROWTH -Clink-arg=-sEXPORTED_RUNTIME_METHODS=FS,callMain -Clink-arg=-sMODULARIZE -Clink-arg=-sEXPORT_ES6' cargo install --root $(WASM_INSTALL_PREFIX) --target wasm32-unknown-emscripten --path crates/solidity
	echo '{ "type": "module" }' > $(WASM_INSTALL_PREFIX)/target/wasm32-unknown-emscripten/release/package.json

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

test-wasm: install-wasm
	npm run test:wasm

bench-prepare: install-bin
	cargo criterion --bench prepare --features bench-evm,bench-pvm --message-format=json \
	| criterion-table > crates/benchmarks/PREPARE.md

bench-execute: install-bin
	cargo criterion --bench execute --features bench-evm,bench-pvm --message-format=json \
	| criterion-table > crates/benchmarks/EXECUTE.md

bench-extensive: install-bin
	cargo criterion --all --all-features --message-format=json \
	| criterion-table > crates/benchmarks/BENCHMARKS.md

bench-quick: install-bin
	cargo criterion --all --features bench-evm

bench: install-bin
	cargo criterion --all --features bench-evm,bench-pvm --message-format=json \
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
