.PHONY: install format test test-solidity test-cli test-integration test-workspace clean docs docs-build

RUSTFLAGS_EMSCRIPTEN := \
	-Clink-arg=-sEXPORTED_FUNCTIONS=_main,_free,_malloc \
	-Clink-arg=-sNO_INVOKE_RUN \
	-Clink-arg=-sEXIT_RUNTIME \
	-Clink-arg=-sINITIAL_MEMORY=64MB \
	-Clink-arg=-sTOTAL_MEMORY=3GB \
	-Clink-arg=-sALLOW_MEMORY_GROWTH \
	-Clink-arg=-sEXPORTED_RUNTIME_METHODS=FS,callMain,stringToNewUTF8,cwrap \
	-Clink-arg=-sMODULARIZE \
	-Clink-arg=-sEXPORT_ES6 \
	-Clink-arg=--js-library=js/soljson_interface.js \
	-Clink-arg=--pre-js=js/pre.js

install: install-bin install-npm

install-bin:
	cargo install --path crates/solidity

install-wasm:
	RUSTFLAGS='$(RUSTFLAGS_EMSCRIPTEN)' cargo install --target wasm32-unknown-emscripten --path crates/solidity

install-npm:
	npm install && npm fund

# install-revive: Build and install to the directory specified in REVIVE_INSTALL_DIR
ifeq ($(origin REVIVE_INSTALL_DIR), undefined)
REVIVE_INSTALL_DIR=`pwd`/release/revive-debian
endif
install-revive:
	cargo install --path crates/solidity --root $(REVIVE_INSTALL_DIR)

format:
	cargo fmt --all --check

clippy:
	cargo clippy --all-features --workspace --tests --benches -- --deny warnings --allow dead_code

test: format clippy test-cli test-workspace

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
