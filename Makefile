.PHONY: \
	install \
	install-bin \
	install-npm \
	install-wasm \
	install-llvm-builder \
	install-llvm \
	install-llvm-coverage \
	install-cargo-llvm-cov \
	install-revive-runner \
	format \
	clippy \
	doc \
	book \
	machete \
	test \
	test-integration \
	test-resolc \
	test-yul \
	test-workspace \
	test-wasm \
	test-llvm-builder \
	test-book \
	coverage \
	coverage-reset \
	bench \
	bench-pvm \
	bench-evm \
	bench-resolc \
	bench-yul \
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

# install-llvm with -DLLVM_BUILD_INSTRUMENTED_COVERAGE=On. Shares the
# target-llvm/<env>/ tree with install-llvm; `revive-llvm clean` between
# variants if needed. `JOBS=N` caps thread count.
install-llvm-coverage: install-llvm-builder
	git submodule update --init --recursive --depth 1
	CMAKE_BUILD_PARALLEL_LEVEL=$(JOBS) revive-llvm build --llvm-projects lld --llvm-projects clang --enable-coverage

install-revive-runner:
	cargo install --locked --force --path crates/runner --no-default-features

format:
	cargo fmt --all --check

clippy:
	cargo clippy --all-features --workspace --tests --benches -- --deny warnings

doc:
	cargo doc --all-features --workspace --document-private-items --no-deps

book: test-book
	mdbook serve book --open

machete:
	cargo install cargo-machete
	cargo machete

test: format clippy machete test-workspace install-revive-runner doc test-book

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

install-cargo-llvm-cov:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov --locked
	@rustup component add llvm-tools-preview >/dev/null 2>&1 || true

test-book:
	cargo install mdbook --version 0.5.1 --locked
	mdbook test book

# Coverage over `test-workspace` (excludes revive-llvm-builder).
# Stages HTML under book/src/coverage/ and stamps the chapter in place;
# revert with `make coverage-reset`.
coverage: install install-cargo-llvm-cov
	cargo install mdbook --version 0.5.1 --locked
	cargo llvm-cov clean --workspace
	rm -rf target/coverage
	cargo llvm-cov --workspace \
		--exclude revive-llvm-builder \
		--all-targets \
		--locked \
		--ignore-run-fail \
		--html \
		--output-dir target/coverage
	cargo llvm-cov report --summary-only > target/coverage/summary.txt
	rm -rf book/src/coverage
	mkdir -p book/src/coverage
	mv target/coverage/html book/src/coverage/html
	mv target/coverage/summary.txt book/src/coverage/summary.txt
	@COMMIT=$$(git rev-parse --short HEAD 2>/dev/null || echo unknown) ; \
	 TIMESTAMP=$$(date -u +"%Y-%m-%dT%H:%M:%SZ") ; \
	 COVERED=$$(awk '/^TOTAL/ { print $$10; exit }' target/coverage/summary.txt) ; \
	 [ -n "$$COVERED" ] || COVERED=N/A ; \
	 awk -v commit="$$COMMIT" -v ts="$$TIMESTAMP" -v covered="$$COVERED" \
	     -v link="../coverage/html/index.html" \
	     -f .github/scripts/stamp-coverage-chapter.awk \
	     book/src/developer_guide/coverage.md \
	     > book/src/developer_guide/coverage.md.new
	mv book/src/developer_guide/coverage.md.new book/src/developer_guide/coverage.md
	mdbook build book
	@echo
	@echo "Coverage collected. Run 'make book' to view."
	@echo "Run 'make coverage-reset' before committing."

# Restore the committed chapter and drop the staged HTML.
coverage-reset:
	cargo install mdbook --version 0.5.1 --locked
	git checkout -- book/src/developer_guide/coverage.md
	rm -rf book/src/coverage
	mdbook build book
	@echo "Coverage chapter restored."

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
	cargo criterion --package revive-benchmarks --bench parse --message-format=json \
	| criterion-table > crates/benchmarks/BENCHMARKS_PARSE_M4PRO.md
	cargo criterion --package revive-benchmarks --bench lower --message-format=json \
	| criterion-table > crates/benchmarks/BENCHMARKS_LOWER_M4PRO.md

clean:
	cargo clean ; \
	revive-llvm clean ; \
	rm -rf node_modules ; \
	rm -rf crates/resolc/src/tests/cli/artifacts ; \
	cargo uninstall resolc ; \
	cargo uninstall revive-llvm-builder ;
	mdbook clean book
