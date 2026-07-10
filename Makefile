.PHONY: \
	install \
	install-bin \
	install-npm \
	install-wasm \
	install-llvm-builder \
	install-llvm \
	install-llvm-coverage \
	install-cargo-llvm-cov \
	install-mdbook \
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
	coverage-book \
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

# Build an instrumented LLVM, enabling LLVM C++ coverage.
# Shares the `target-llvm/<env>/` tree with `install-llvm`.
# Run `revive-llvm clean` between variants. `JOBS=N` caps build parallelism.
install-llvm-coverage: install-llvm-builder
	git submodule update --init --recursive --depth 1
	CMAKE_BUILD_PARALLEL_LEVEL=$(JOBS) revive-llvm build --llvm-projects lld --llvm-projects clang --enable-coverage

install-cargo-llvm-cov:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov --locked
	@rustup component add llvm-tools-preview >/dev/null 2>&1 || true

install-mdbook:
	cargo install mdbook --version 0.5.1 --locked

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
	cargo test --package revive-integration --features newyork

test-resolc: install
	cargo test --package resolc --all-targets

test-yul:
	cargo test --package revive-yul --all-targets

test-workspace: install
	cargo test --workspace --all-targets --exclude revive-llvm-builder
	cargo test --package revive-integration --features newyork

test-wasm: install-wasm
	npm run test:wasm

test-llvm-builder:
	@echo "warning: the llvm-builder tests will take many hours"
	cargo test --package revive-llvm-builder -- --test-threads=1

test-book: install-mdbook
	mdbook test book

coverage: install-cargo-llvm-cov
	cargo llvm-cov clean --workspace
	rm -rf target/coverage
# Use `--no-report` in order to merge the two reports at the later step.
	PATH="$(CURDIR)/target/llvm-cov-target/debug:$$PATH" \
	cargo llvm-cov --no-report --workspace \
		--exclude revive-llvm-builder \
		--all-targets \
		--locked \
		--ignore-run-fail
	PATH="$(CURDIR)/target/llvm-cov-target/debug:$$PATH" \
	cargo llvm-cov --no-report --package revive-integration \
		--features newyork \
		--locked \
		--ignore-run-fail
	cargo llvm-cov report --html --output-dir target/coverage
	cargo llvm-cov report > target/coverage/html/report.txt
# Slice the report's header and the `TOTAL` row into a summary file.
	{ head -n 2 target/coverage/html/report.txt; tail -n 1 target/coverage/html/report.txt; } \
		| tee target/coverage/summary.txt
# The LLVM C++ report requires an instrumented LLVM and resolc rebuilt against it,
# otherwise we skip it. The report is kept separate from the Rust report to prevent
# blending LLVM percentages into resolc's own coverage numbers.
# Raw llvm-profdata/llvm-cov is used (rather than `cargo llvm-cov`) since that
# allows for include-only filtering (e.g. "only llvm/").
	@if "$(LLVM_SYS_221_PREFIX)/bin/llvm-objdump" -h \
		"$(LLVM_SYS_221_PREFIX)/lib/libLLVMCore.a" 2>/dev/null \
		| grep -q __llvm_covmap; then \
		mkdir -p target/coverage-llvm; \
		"$(LLVM_SYS_221_PREFIX)/bin/llvm-profdata" merge -sparse \
			$$(find target/llvm-cov-target -name '*.profraw') \
			-o target/coverage-llvm/llvm.profdata; \
		"$(LLVM_SYS_221_PREFIX)/bin/llvm-cov" report \
			-instr-profile target/coverage-llvm/llvm.profdata \
			target/llvm-cov-target/debug/resolc \
			llvm/ | tee target/coverage-llvm/report.txt; \
	else \
		echo "note: LLVM at LLVM_SYS_221_PREFIX is not instrumented;" \
			"skipping the LLVM C++ coverage report ('make install-llvm-coverage' enables it)."; \
	fi

# Local coverage browsing: runs coverage then stages the report into the
# gitignored book/src/coverage/ and rebuilds the book.
coverage-book: coverage install-mdbook
	rm -rf book/src/coverage
	mkdir -p book/src/coverage
	mv target/coverage/html book/src/coverage/html
	mv target/coverage/summary.txt book/src/coverage/summary.txt
	mdbook build book
	@echo
	@echo "Coverage collected under book/src/coverage/ (gitignored)."
	@echo "Open docs/coverage/html/index.html, or run 'make book' to browse it."

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
