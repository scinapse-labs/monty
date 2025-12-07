.DEFAULT_GOAL := all

.PHONY: .cargo
.cargo: ## Check that cargo is installed
	@cargo --version || echo 'Please install cargo: https://github.com/rust-lang/cargo'

.PHONY: .pre-commit
.pre-commit: ## Check that pre-commit is installed
	@pre-commit -V || echo 'Please install pre-commit: https://pre-commit.com/'

.PHONY: install
install: .cargo .pre-commit ## Install the package, dependencies, and pre-commit for local development
	cargo check
	pre-commit install --install-hooks

.PHONY: format-rs
format-rs:  ## Format Rust code with fmt
	@cargo fmt --version
	cargo fmt --all

.PHONY: format-py
format-py: ## Format Python code - WARNING be careful about this command as it may modify code and break tests silently!
	uv run ruff format
	uv run ruff check --fix --fix-only

.PHONY: format
format: format-rs ## Format Rust code, this does not format Python code as we have to be careful with that

.PHONY: lint-rs
lint-rs:  ## Lint Rust code with fmt and clippy
	@cargo clippy --version
	cargo clippy --tests --bench main -- -D warnings -A incomplete_features
	cargo clippy --tests --all-features -- -D warnings -A incomplete_features

.PHONY: lint-py
lint-py: ## Lint Python code with ruff
	uv run ruff format --check
	uv run ruff check
	uv run basedpyright

.PHONY: lint
lint: lint-rs lint-py ## Lint the code with ruff and clippy

.PHONY: format-lint-rs
format-lint-rs: format-rs lint-rs ## Format and lint Rust code with fmt and clippy

.PHONY: test
test: ## Run tests with dec-ref-check enabled
	cargo test --features dec-ref-check

.PHONY: test-ref-counting
test-ref-counting: ## Run tests with ref-counting enabled
	cargo test --features ref-counting

.PHONY: complete-tests
complete-tests: ## Fill in incomplete test expectations using CPython
	uv run scripts/complete_tests.py

.PHONY: bench
bench: ## Run benchmarks
	cargo bench --bench main

.PHONY: profile
profile: ## Profile the code with pprof and generate flamegraphs
	cargo bench --bench main --profile profiling -- --profile-time=10
	uv run scripts/flamegraph_to_text.py

.PHONY: all
all: lint test
