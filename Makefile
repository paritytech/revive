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
	coverage-llvm-report \
	coverage-browse \
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

test-book:
	cargo install mdbook --version 0.5.1 --locked
	mdbook test book

# The active run's report directory. Resolved at recipe time
# from the stamp that `coverage` records next to the profiles.
# Reports accumulate under coverage-reports/<stamp>/.
COVERAGE_REPORTS_DIR = coverage-reports/$$(cat target/llvm-cov-target/coverage-stamp)

# Envs are used for reducing disk and memory usage, and preventing errors
# arising because of it. See ./book/src/developer_guide/coverage.md on
# more details and troubleshooting techniques.
coverage: export CARGO_PROFILE_DEV_DEBUG = 0
coverage: export LLVM_PROFILE_FILE_NAME = revive-%4m.profraw
coverage: install-cargo-llvm-cov
	cargo llvm-cov clean --workspace
	mkdir -p target/llvm-cov-target
	date '+%Y-%m-%d_%H-%M' > target/llvm-cov-target/coverage-stamp
# Use `--no-report` in order to merge the two Rust reports at the later step.
# Benches are excluded (i.e. `--all-targets` is not used) since they run duplicate
# coverage the tests already provide, while their binaries noticeably inflate each
# report pass with instrumented LLVM.
	PATH="$(CURDIR)/target/llvm-cov-target/debug:$$PATH" \
	cargo llvm-cov --no-report --workspace \
		--exclude revive-llvm-builder \
		--lib --bins --tests \
		--locked \
		--ignore-run-fail
	PATH="$(CURDIR)/target/llvm-cov-target/debug:$$PATH" \
	cargo llvm-cov --no-report --package revive-integration \
		--features newyork \
		--locked \
		--ignore-run-fail
# Exclude the llvm/ and target-llvm/ trees from this workspace-only report
# (see `coverage-llvm-report` for an LLVM report).
	cargo llvm-cov report --html --output-dir $(COVERAGE_REPORTS_DIR)/revive \
		--ignore-filename-regex '^$(CURDIR)/(llvm|target-llvm)/'
	cargo llvm-cov report \
		--ignore-filename-regex '^$(CURDIR)/(llvm|target-llvm)/' \
		> $(COVERAGE_REPORTS_DIR)/revive/report.txt
# Slice the report's header and the `TOTAL` row into a summary file.
	{ head -n 2 $(COVERAGE_REPORTS_DIR)/revive/report.txt; \
	  tail -n 1 $(COVERAGE_REPORTS_DIR)/revive/report.txt; } \
		| tee $(COVERAGE_REPORTS_DIR)/revive/summary.txt
	@echo "revive coverage report: file://$(CURDIR)/$(COVERAGE_REPORTS_DIR)/revive/html/index.html"
	@if "$(LLVM_SYS_221_PREFIX)/bin/llvm-objdump" -h \
		"$(LLVM_SYS_221_PREFIX)/lib/libLLVMCore.a" 2>/dev/null \
		| grep -q __llvm_covmap; then \
		echo "note: instrumented LLVM detected. Run 'make coverage-llvm-report'" \
			"to generate the LLVM C++ coverage report from this run."; \
	fi

# Render the LLVM C++ coverage report that a `make coverage` run against an
# instrumented LLVM enabled. This is kept separate from the Rust report to
# prevent blending LLVM percentages into resolc's own coverage numbers.
coverage-llvm-report:
	@"$(LLVM_SYS_221_PREFIX)/bin/llvm-objdump" -h \
		"$(LLVM_SYS_221_PREFIX)/lib/libLLVMCore.a" 2>/dev/null \
		| grep -q __llvm_covmap || { \
		echo "error: no instrumented LLVM at LLVM_SYS_221_PREFIX='$(LLVM_SYS_221_PREFIX)'" \
			"('make install-llvm-coverage' enables it)."; \
		exit 1; \
	}
	@"$(LLVM_SYS_221_PREFIX)/bin/llvm-config" --system-libs | grep -q -- -lz || { \
		echo "error: the instrumented LLVM at LLVM_SYS_221_PREFIX lacks zlib." \
			"Install it and rebuild LLVM with 'make install-llvm-coverage'."; \
		exit 1; \
	}
	@find target/llvm-cov-target -name '*.profraw' 2>/dev/null | grep -q . || { \
		echo "error: no coverage profiles found. Run 'make coverage' first."; \
		exit 1; \
	}
	@test -f target/llvm-cov-target/coverage-stamp || { \
		echo "error: no recorded coverage run stamp. Run 'make coverage' first."; \
		exit 1; \
	}
	mkdir -p target/coverage-llvm
# Raw llvm-profdata/llvm-cov is used (rather than `cargo llvm-cov`) since
# that allows include-only filtering (e.g. "only llvm/").
	"$(LLVM_SYS_221_PREFIX)/bin/llvm-profdata" merge -sparse \
		--failure-mode=all \
		$$(find target/llvm-cov-target -name '*.profraw') \
		-o target/coverage-llvm/llvm.profdata
	"$(LLVM_SYS_221_PREFIX)/bin/llvm-cov" show -format=html \
		-output-dir $(COVERAGE_REPORTS_DIR)/llvm/html \
		-instr-profile target/coverage-llvm/llvm.profdata \
		target/llvm-cov-target/debug/resolc \
		llvm/
	@echo "LLVM C++ coverage report: file://$(CURDIR)/$(COVERAGE_REPORTS_DIR)/llvm/html/index.html"

# Local coverage browsing.
# Prints clickable links to the HTML reports of every recorded coverage run.
coverage-browse:
	@runs="$$(ls -1r coverage-reports 2>/dev/null)"; \
	if [ -z "$$runs" ]; then \
		echo "note: no coverage reports found. Run 'make coverage' first."; \
	else \
		for run in $$runs; do \
			echo "$$run:"; \
			test -f "coverage-reports/$$run/revive/html/index.html" && \
				echo "  revive:   file://$(CURDIR)/coverage-reports/$$run/revive/html/index.html" && \
				echo; \
			test -f "coverage-reports/$$run/llvm/html/index.html" && \
				echo "  LLVM C++: file://$(CURDIR)/coverage-reports/$$run/llvm/html/index.html" && \
				echo; \
			true; \
		done; \
	fi

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
