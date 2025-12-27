# Axiograph Rust Build & Test Makefile

.PHONY: all build test test-unit test-integration test-e2e test-all clean doc check fmt lint

# Default target
all: check test

# Build all crates
build:
	cargo build --workspace

# Build in release mode
release:
	cargo build --workspace --release

# Run all tests
test: test-unit test-integration

# Unit tests only
test-unit:
	@echo "=== Unit Tests ==="
	cargo test --workspace --lib

# Integration tests
test-integration:
	@echo "=== Integration Tests ==="
	cargo test --test integration_tests

# E2E tests for LLM sync
test-llm-sync:
	@echo "=== LLM Sync E2E Tests ==="
	cargo test -p axiograph-llm-sync --test e2e_tests

# Reconciliation tests
test-reconciliation:
	@echo "=== Reconciliation Tests ==="
	cargo test -p axiograph-llm-sync --test reconciliation_tests

# Reconciliation persistence/format tests
test-reconciliation-format:
	@echo "=== Reconciliation Format Tests ==="
	cargo test -p axiograph-llm-sync --test reconciliation_format_tests

# Path verification tests
test-path-verification:
	@echo "=== Path Verification Tests ==="
	cargo test -p axiograph-llm-sync --test path_verification_tests

# PathDB tests
test-pathdb:
	@echo "=== PathDB Tests ==="
	cargo test -p axiograph-pathdb

# Storage tests
test-storage:
	@echo "=== Storage Tests ==="
	cargo test -p axiograph-storage

# All tests including slow ones
test-all:
	@echo "=== All Tests (including slow) ==="
	cargo test --workspace -- --include-ignored

# Doc tests only
test-doc:
	@echo "=== Doc Tests ==="
	cargo test --doc --workspace

# Check without building
check:
	cargo check --workspace --all-features

# Format code
fmt:
	cargo fmt --all

# Format check
fmt-check:
	cargo fmt --all -- --check

# Lint with clippy
lint:
	cargo clippy --workspace --all-features -- -D warnings

# Generate documentation
doc:
	cargo doc --workspace --no-deps --open

# Clean build artifacts
clean:
	cargo clean

# Run specific crate tests
test-dsl:
	cargo test -p axiograph-dsl

# Continuous testing (requires cargo-watch)
watch:
	cargo watch -x "test --workspace --lib"

# Coverage report (requires cargo-tarpaulin)
coverage:
	cargo tarpaulin --workspace --out Html

# Quick check cycle
quick: fmt check test-unit

# CI pipeline
ci: fmt-check check lint test test-doc

# Print test summary
summary:
	@echo ""
	@echo "=== Axiograph Test Summary ==="
	@echo ""
	@echo "Unit tests:            cargo test --workspace --lib"
	@echo "Integration:           cargo test --test integration_tests"
	@echo "LLM Sync E2E:          cargo test -p axiograph-llm-sync --test e2e_tests"
	@echo "Reconciliation:        cargo test -p axiograph-llm-sync --test reconciliation_tests"
	@echo "Reconciliation Format: cargo test -p axiograph-llm-sync --test reconciliation_format_tests"
	@echo "Path Verification:     cargo test -p axiograph-llm-sync --test path_verification_tests"
	@echo "PathDB:                cargo test -p axiograph-pathdb --test pathdb_tests"
	@echo "Storage:               cargo test -p axiograph-storage"
	@echo "Doc tests:             cargo test --doc"
	@echo "All (with slow):       cargo test -- --include-ignored"
	@echo ""

# Full reconciliation test suite
test-reconciliation-all: test-reconciliation test-reconciliation-format test-path-verification
	@echo "=== All Reconciliation Tests Passed ==="
