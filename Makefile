.PHONY: install format test test-solidity test-cli test-integration test-workspace test-wasm clean docs docs-build

RUSTFLAGS_EMSCRIPTEN := \
	-C link-arg=-sEXPORTED_FUNCTIONS=_main,_free,_malloc \
	-C link-arg=-sNO_INVOKE_RUN=1 \
	-C link-arg=-sEXIT_RUNTIME=1 \
	-C link-arg=-sALLOW_MEMORY_GROWTH=1 \
	-C link-arg=-sEXPORTED_RUNTIME_METHODS=FS,callMain,stringToNewUTF8 \
	-C link-arg=-sMODULARIZE=1 \
	-C link-arg=-sEXPORT_NAME=createRevive \
	-C link-arg=-sWASM_ASYNC_COMPILATION=0 \
	-C link-arg=-sDYNAMIC_EXECUTION=0 \
	-C link-arg=-sALLOW_TABLE_GROWTH=1 \
	-C link-arg=--js-library=js/embed/soljson_interface.js \
	-C link-arg=--pre-js=js/embed/pre.js \
	-C link-arg=-sNODEJS_CATCH_EXIT=0 \
	-C link-arg=-sDISABLE_EXCEPTION_CATCHING=0 \
	-C opt-level=3

install: install-bin install-npm

install-bin:
	cargo install --path crates/solidity

install-npm:
	npm install && npm fund

install-wasm: install-npm
	RUSTFLAGS='$(RUSTFLAGS_EMSCRIPTEN)' cargo build --target wasm32-unknown-emscripten -p revive-solidity --release --no-default-features

test-wasm: install-wasm
	npm run test:wasm

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

clean:
	cargo clean ; \
	rm -rf node_modules ; \
	rm -rf crates/solidity/src/tests/cli-tests/artifacts ; \
	cargo uninstall revive-solidity ; \
	rm -f package-lock.json ; \
	rm -rf js/dist ; \
	rm -f js/src/resolc.{wasm,js}
