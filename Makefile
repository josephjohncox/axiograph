# shellcheck disable=SC1089,SC2046,SC2035
# This is GNU Make syntax; shellcheck otherwise parses Make variables/recipes as a shell script.
# ============================================================================
# Axiograph Master Makefile
# ============================================================================
# 
# Builds the complete Axiograph system:
# - Rust crates (PathDB, ingestion, LLM sync, CLI)
# - Lean checker (certificate/spec verification)
# - Example binaries and demos
#
# Usage:
#   make all          - Build everything
#   make rust         - Build Rust crates
#   make lean         - Build Lean checker
#   make demo         - Run end-to-end demo
#   make test         - Run all tests
#   make clean        - Clean build artifacts

.PHONY: all all-exe rust lean lean-update lean-cache lean-system-cc lean-exe verify-lean-cert \
	verify-lean-theory verify-lean-e2e-category-kernel-v3 verify-lean-semantic-vcs verify-axi-store \
	verify-lean-axi-schema-v1 \
	verify-lean-axi-v1 \
	verify-identity-parity \
	verify-w02-compiler verify-regulated-shipment \
	verify-axi-parse-e2e \
	verify-verus \
	verify-lean-resolution-v2 verify-lean-normalize-path-v2 verify-lean-path-equiv-v2 verify-lean-delta-f-v1 \
	verify-lean-indexed-path-theory \
	verify-lean-e2e-axi-well-typed-v1 \
	verify-lean-e2e-axi-constraints-ok-v1 \
	verify-lean-e2e-query-result-module-v4 \
	verify-lean-e2e-resolution-v2 verify-lean-e2e-normalize-path-v2 verify-lean-e2e-path-equiv-v2 verify-lean-e2e-path-equiv-congr-v2 verify-lean-e2e-delta-f-v1 \
	verify-lean-certificates verify-lean-certificate-rejections verify-lean-e2e-suite \
	rust-test-semantics check-rust-toolchain check-clean-source-manifest check-example-catalog rust-fmt-check rust-test-locked rust-test-all-targets-features check-cli-feature-matrix \
	verify-release-fixtures verify-release-packaging rehearse-release-publication \
	release-gate check-no-unsafe verify-semantics verify-canonical-spine test-semantics test-backend-containers \
	viz-install viz-build viz-dev \
	demo test clean install help

# ============================================================================
# Configuration
# ============================================================================

RUST_DIR := rust
LEAN_DIR := lean
EXAMPLES_DIR := examples
BUILD_DIR := build
BIN_DIR := bin

# Detect OS without conditional directives so this file remains parseable by
# shell-oriented lint wrappers used by the repository harness.
UNAME := $(shell uname)
DYLIB_EXT := $(shell uname | sed -e 's/^Darwin$$/dylib/' -e 's/^[^d].*/so/')
SHARED_FLAG := $(shell uname | sed -e 's/^Darwin$$/-dynamiclib/' -e 's/^[^-].*/-shared/')

# Rust configuration
CARGO := cargo
CARGO_OPTS := --release
CARGO_FEATURES ?=
RUST_VERSION := $(shell python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')

# Lean configuration (optional)
LAKE := lake

# On macOS, Lean's bundled clang needs a valid macOS SDK to link executables.
# We keep Lean's toolchain (so it can find its bundled libs like `libgmp.a`),
# but we provide `SDKROOT` via `xcrun` to point it at the system SDK.
MACOSX_DEPLOYMENT_TARGET ?= 13.0
# shellcheck disable=SC2034
Darwin_LEAN_ENV=SDKROOT="$$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)" MACOSX_DEPLOYMENT_TARGET="$(MACOSX_DEPLOYMENT_TARGET)"
# shellcheck disable=SC2034
LEAN_ENV=$($(UNAME)_LEAN_ENV)

# ============================================================================
# Default target
# ============================================================================

all: dirs rust lean binaries
	@echo ""
	@echo "╔══════════════════════════════════════════════════════════════╗"
	@echo "║              AXIOGRAPH BUILD COMPLETE                        ║"
	@echo "╚══════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "Binary available in $(BIN_DIR)/"
	@echo "  - axiograph          : Main CLI tool"
	@echo ""
	@echo "Run 'make demo' to see end-to-end example"

all-exe: all lean-exe

# ============================================================================
# Directory setup
# ============================================================================

dirs:
	@mkdir -p $(BUILD_DIR)
	@mkdir -p $(BIN_DIR)

# ============================================================================
# Rust Build
# ============================================================================

rust: dirs
	@echo "━━━ Building Rust crates ━━━"
	cd $(RUST_DIR) && $(CARGO) build --workspace $(CARGO_OPTS) $(CARGO_FEATURES)
	@echo "✓ Rust build complete"

rust-debug: dirs
	@echo "━━━ Building Rust crates (debug) ━━━"
	cd $(RUST_DIR) && $(CARGO) build --workspace $(CARGO_FEATURES)
	@echo "✓ Rust debug build complete"

rust-test:
	@echo "━━━ Running Rust tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test --all
	@echo "✓ Rust tests complete"

check-rust-toolchain:
	@echo "━━━ Checking exact Rust toolchain $(RUST_VERSION) ━━━"
	@python3 -c 'import tomllib; d=tomllib.load(open("rust-toolchain.toml", "rb")); assert set(d) == {"toolchain"}; t=d["toolchain"]; assert t == {"channel": "$(RUST_VERSION)", "profile": "minimal", "components": ["cargo", "clippy", "rustfmt"]}'
	@test "$$(rustc --version --verbose | awk '/^release:/ { print $$2 }')" = "$(RUST_VERSION)" || { \
		echo "error: release gate requires rustc $(RUST_VERSION)"; \
		rustc --version; \
		exit 1; \
	}
	@test "$$(cargo --version | awk '{ print $$2 }')" = "$(RUST_VERSION)" || { \
		echo "error: release gate requires cargo $(RUST_VERSION)"; \
		cargo --version; \
		exit 1; \
	}
	@rustfmt --version >/dev/null
	@cargo clippy --version >/dev/null
	@echo "✓ Rust compiler, Cargo, rustfmt, and Clippy come from exact toolchain $(RUST_VERSION)"

check-clean-source-manifest:
	@echo "━━━ Proving release inputs come from one clean Git checkout ━━━"
	python3 scripts/generate_release_source_manifest.py --check-only >/dev/null
	@echo "✓ Clean checkout produced a canonical exact-byte source manifest"

check-example-catalog:
	@echo "━━━ Validating example catalog ━━━"
	python3 examples/check_catalog.py
	@echo "✓ Example catalog is valid"

check-greenfield-surface:
	@echo "━━━ Rejecting retired commands, aliases, and stale shell workflows ━━━"
	python3 scripts/check_greenfield_surface.py
	@echo "✓ Greenfield command surface is internally consistent"

rust-fmt-check:
	@echo "━━━ Checking Rust formatting ━━━"
	cd $(RUST_DIR) && $(CARGO) fmt --all --check
	@echo "✓ Rust formatting is clean"

rust-test-locked:
	@echo "━━━ Running full locked Rust workspace tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test --workspace --locked
	@echo "✓ Full locked Rust workspace tests complete"

rust-test-all-targets-features:
	@echo "━━━ Testing every Rust workspace target with every feature ━━━"
	cd $(RUST_DIR) && $(CARGO) test --workspace --all-targets --all-features --locked
	@echo "✓ Every Rust workspace target and feature combination compiled and tested"

check-cli-feature-matrix:
	@echo "━━━ Checking CLI feature matrix ━━━"
	cd $(RUST_DIR) && $(CARGO) check -p axiograph-cli --no-default-features --locked
	@set -e; \
	for _feature in \
		repl-rustyline llm-ollama llm-openai llm-anthropic \
		profiling proposal-adapter-http; do \
		cd $(RUST_DIR) && $(CARGO) check -p axiograph-cli \
			--no-default-features --features "$$_feature" --locked; \
		cd ..; \
	done
	@echo "✓ CLI feature matrix complete"

rust-test-semantics:
	@echo "━━━ Running Rust semantics tests (axiograph-pathdb) ━━━"
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb
	@echo "✓ Rust semantics tests complete"

check-no-unsafe:
	@echo "━━━ Auditing first-party Rust for unsafe code ━━━"
	python3 scripts/check_no_unsafe.py
	cd $(RUST_DIR) && $(CARGO) check --workspace --all-targets --all-features --locked
	@echo "✓ First-party Rust is compiler-enforced safe code"

# ============================================================================
# Lean Build (manifest refresh explicit; cache/build/verification fail closed)
# ============================================================================

lean-update: dirs
	@echo "━━━ Updating pinned Lean dependency manifest explicitly ━━━"
	@ command -v $(LAKE) >/dev/null 2>&1 || { \
		echo "error: lake (Lean) not found; cannot update Lean dependencies"; \
		exit 127; \
	}
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) update
	@echo "✓ Lean manifest update complete; review lean/lake-manifest.json"

lean-cache: dirs
	@echo "━━━ Fetching Lean cache from the checked-in manifest ━━━"
	@ command -v $(LAKE) >/dev/null 2>&1 || { \
		echo "error: lake (Lean) not found; required Lean cache cannot be fetched"; \
		exit 127; \
	}
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe cache get
	@echo "✓ Lean cache matches the checked-in manifest"

lean: dirs lean-cache
	@echo "━━━ Building Lean checker ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && echo "✓ Lean build complete"; \
	else \
		echo "error: lake (Lean) not found; required Lean build cannot run"; \
		echo "Install via elan: https://leanprover-community.github.io/get_started.html"; \
		exit 127; \
	fi

lean-system-cc: lean

lean-exe: dirs lean-cache
	@echo "━━━ Building Lean checker executable (axiograph_verify) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		if [ "$(UNAME)" = "Darwin" ]; then \
			sdk="$$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"; \
			if [ -z "$$sdk" ]; then \
				echo "error: failed to locate macOS SDK via xcrun (needed to link Lean executables)."; \
				echo "hint: install Xcode Command Line Tools: xcode-select --install"; \
				exit 2; \
			fi; \
		fi; \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_verify && \
			cp .lake/build/bin/axiograph_verify ../$(BIN_DIR)/axiograph_verify && \
			echo "✓ Lean executable built + installed to $(BIN_DIR)/axiograph_verify"; \
	else \
		echo "error: lake (Lean) not found; required checker executable cannot be built"; \
		echo "Install via elan: https://leanprover-community.github.io/get_started.html"; \
		exit 127; \
	fi

verify-lean-cert: lean-exe
	@echo "━━━ Running Lean checker executable (custom cert) ━━━"
	@ if [ -z "$(CERT)" ]; then \
		echo "error: set CERT=/path/to/certificate.json (and optional AXI=/path/to/module.axi)"; \
		exit 2; \
	fi
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		if [ -n "$(AXI)" ]; then \
			cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_verify "$(abspath $(AXI))" "$(abspath $(CERT))" && echo "✓ Lean verified cert: $(CERT)"; \
		else \
			cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_verify "$(abspath $(CERT))" && echo "✓ Lean verified cert: $(CERT)"; \
		fi; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-resolution-v2: lean
	@echo "━━━ Running Lean checker (resolution cert v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../fixtures/certificates/resolution_v2.json && echo "✓ Lean checker ran (resolution v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-normalize-path-v2: lean
	@echo "━━━ Running Lean checker (normalize_path cert v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../fixtures/certificates/normalize_path_v2.json && echo "✓ Lean checker ran (normalize_path v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-path-equiv-v2: lean
	@echo "━━━ Running Lean checker (path_equiv cert v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../fixtures/certificates/path_equiv_v2.json && echo "✓ Lean checker ran (path_equiv v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-delta-f-v1: lean
	@echo "━━━ Running Lean checker (delta_f cert v1) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../fixtures/certificates/delta_f_v1.json && echo "✓ Lean checker ran (delta_f v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-certificates: lean
	@echo "━━━ Running Lean checker (certificate fixtures) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../fixtures/verification/rewrite_rules_anchor_v1.axi ../fixtures/certificates/*.json && echo "✓ Lean verified certificate fixtures"; \
	else \
		echo "error: lake (Lean) not found; required certificate checks cannot run"; \
		exit 127; \
	fi

verify-lean-certificate-rejections: lean-exe
	@echo "━━━ Running approved-checker adversarial rejection tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli verifier_bridge::tests -- --nocapture
	@echo "✓ Approved checker accepted exact V4 and rejected malformed, altered-digest, forged, extra, missing, duplicate, and truncated inputs"

verify-lean-theory: dirs
	@echo "━━━ Verifying finite category/dependent/groupoid theory ━━━"
	@ command -v $(LAKE) >/dev/null 2>&1 || { \
		echo "error: lake (Lean) not found; finite theory verification cannot run"; \
		exit 127; \
	}
	@if rg -n '^[[:space:]]*(axiom|unsafe[[:space:]]+def|sorry)([[:space:]]|$$)' $(LEAN_DIR)/Axiograph; then \
		echo "error: first-party Lean theory contains an axiom, sorry, or unsafe definition"; \
		exit 1; \
	fi
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_finite_theory_tests
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_finite_theory_tests
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-kernel --locked
	$(MAKE) verify-lean-e2e-category-kernel-v3
	$(MAKE) verify-lean-indexed-path-theory
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb --test runtime_theory_checker_tests --locked
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli runtime_theory --locked
	@echo "✓ Finite typed theory accepted positive witnesses and rejected adversarial projections, equations, refinements, bounds, and explanations"

verify-lean-e2e-category-kernel-v3: dirs
	@echo "━━━ Verifying the exact-byte category-kernel certificate end to end ━━━"
	@mkdir -p $(BUILD_DIR)/category-kernel
	./scripts/check_category_kernel_conformance.sh
	$(CARGO) run --quiet --manifest-path $(RUST_DIR)/Cargo.toml -p axiograph-kernel \
		--example emit_category_kernel_certificate --locked -- \
		$(EXAMPLES_DIR)/regulated_shipment/RegulatedShipment.axi RegulatedShipment \
		> $(BUILD_DIR)/category-kernel/regulated-shipment.json
	$(LEAN_DIR)/.lake/build/bin/axiograph_verify \
		$(EXAMPLES_DIR)/regulated_shipment/RegulatedShipment.axi \
		$(BUILD_DIR)/category-kernel/regulated-shipment.json
	@set -e; \
	for _tamper in saturation presentation congruence groupoid; do \
		$(CARGO) run --quiet --manifest-path $(RUST_DIR)/Cargo.toml -p axiograph-kernel \
			--example emit_category_kernel_certificate --locked -- \
			$(EXAMPLES_DIR)/regulated_shipment/RegulatedShipment.axi RegulatedShipment \
			--tamper-$$_tamper \
			> $(BUILD_DIR)/category-kernel/regulated-shipment-$$_tamper-tampered.json; \
		if $(LEAN_DIR)/.lake/build/bin/axiograph_verify \
			$(EXAMPLES_DIR)/regulated_shipment/RegulatedShipment.axi \
			$(BUILD_DIR)/category-kernel/regulated-shipment-$$_tamper-tampered.json \
			> $(BUILD_DIR)/category-kernel/$$_tamper-tampered.stdout \
			2> $(BUILD_DIR)/category-kernel/$$_tamper-tampered.stderr; then \
			echo "error: tampered category-kernel $$_tamper certificate was accepted"; \
			exit 1; \
		fi; \
	done
	@echo "✓ Rust and Lean agreed on indexed category/groupoid paths; Lean rejected saturation, presentation, congruence, and normalization-trace tampering"

verify-lean-semantic-vcs: dirs
	@echo "━━━ Verifying semantic VCS plan contracts against Lean theory ━━━"
	@ command -v $(LAKE) >/dev/null 2>&1 || { \
		echo "error: lake (Lean) not found; semantic VCS verification cannot run"; \
		exit 127; \
	}
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli semantic_merge_lattice -- --nocapture
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_semantic_vcs_check
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_semantic_vcs_check \
		../examples/semantic_merge/plant_clean_merge_lean.json \
		../examples/semantic_merge/plant_clean_rebase_lean.json
	@set -e; for payload in \
		examples/semantic_merge/plant_conflict_merge_lean.json \
		examples/semantic_merge/plant_blocked_rebase_lean.json \
		examples/semantic_merge/plant_failed_transport_no_blocker_lean.json \
		examples/semantic_merge/adversarial_cross_lineage_merge_lean.json \
		examples/semantic_merge/adversarial_dropped_ref_merge_lean.json \
		examples/semantic_merge/adversarial_missing_target_rebase_lean.json; do \
		if (cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_semantic_vcs_check ../$$payload >/dev/null 2>&1); then \
			echo "error: adversarial semantic VCS payload unexpectedly verified: $$payload"; \
			exit 1; \
		fi; \
	done
	@echo "✓ Rust semantic-plan tests and Lean conformance fixtures completed"

verify-axi-store: dirs
	@echo "━━━ Verifying bounded AxiStore and authenticated .axpd cutover ━━━"
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-store -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb --test materialization_tests -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-storage -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli --test db_server_e2e -- --nocapture
	@echo "✓ AxiStore limits, crash, CAS, checksum, receipt, hydration, and server gates passed"

verify-canonical-spine: dirs check-no-unsafe verify-lean-theory
	@echo "━━━ Verifying canonical semantic spine V1 ━━━"
	cd $(RUST_DIR) && $(CARGO) fmt --check
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb runtime_theory -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli prepared_query -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli semantic_merge_lattice -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli --test examples_e2e software_authoring -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli embeddings -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-projections -- --nocapture
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-cli --test projection_cli_e2e -- --nocapture
	@ command -v $(LAKE) >/dev/null 2>&1 || { \
		echo "error: lake (Lean) not found; canonical semantic spine verification cannot run"; \
		exit 127; \
	}
	cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph.SemanticVCS
	$(MAKE) verify-lean-semantic-vcs
	git diff --check
	@echo "✓ Canonical semantic spine V1 gate complete"

verify-lean-axi-schema-v1: lean
	@echo "━━━ Parsing canonical schema .axi corpus (Lean) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/SchemaV1ParseMain.lean ../examples/economics/EconomicFlows.axi ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/SchemaV1ParseMain.lean ../examples/ontology/SchemaEvolution.axi ) && \
		echo "✓ Lean parsed canonical schema corpus"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run parser"; \
	fi

verify-lean-axi-v1: lean
	@echo "━━━ Parsing canonical .axi corpus (Lean, axi_v1) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/economics/EconomicFlows.axi ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/learning/MachinistLearning.axi ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/ontology/SchemaEvolution.axi ) && \
		echo "✓ Lean parsed canonical corpus (axi_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run parser"; \
	fi

verify-w02-compiler:
	@echo "━━━ W02 exact-byte canonical compiler and Rust ↔ Lean conformance ━━━"
	@./scripts/check_w02_compiler_conformance.sh

verify-regulated-shipment: dirs
	@echo "━━━ Verifying the primary regulated-shipment usefulness workflow ━━━"
	@run_dir="$$(mktemp -d "$(BUILD_DIR)/regulated-shipment.XXXXXX")"; \
		./examples/regulated_shipment/run_regulated_shipment_workflow.sh "$$run_dir"
	@echo "✓ Regulated shipment bound exact finite-query receipts into typed category/refinement/transport/merge gates"

verify-axi-parse-e2e: lean
	@echo "━━━ Parsing canonical .axi corpus (Rust ↔ Lean, axi_v1) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-dsl --bin axiograph_parse_axi_v1 -- ../examples/economics/EconomicFlows.axi > ../$(BUILD_DIR)/axi_v1_rust_economic.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/economics/EconomicFlows.axi > ../$(BUILD_DIR)/axi_v1_lean_economic.txt ) && \
		diff -u $(BUILD_DIR)/axi_v1_rust_economic.txt $(BUILD_DIR)/axi_v1_lean_economic.txt && \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-dsl --bin axiograph_parse_axi_v1 -- ../examples/learning/MachinistLearning.axi > ../$(BUILD_DIR)/axi_v1_rust_learning.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/learning/MachinistLearning.axi > ../$(BUILD_DIR)/axi_v1_lean_learning.txt ) && \
		diff -u $(BUILD_DIR)/axi_v1_rust_learning.txt $(BUILD_DIR)/axi_v1_lean_learning.txt && \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-dsl --bin axiograph_parse_axi_v1 -- ../examples/ontology/SchemaEvolution.axi > ../$(BUILD_DIR)/axi_v1_rust_ontology.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/ontology/SchemaEvolution.axi > ../$(BUILD_DIR)/axi_v1_lean_ontology.txt ) && \
		diff -u $(BUILD_DIR)/axi_v1_rust_ontology.txt $(BUILD_DIR)/axi_v1_lean_ontology.txt && \
		echo "✓ Rust and Lean parsers agree on the canonical corpus (axi_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run parse e2e"; \
	fi

verify-identity-parity: lean dirs
	@echo "━━━ AXIOGRAPH-ID parity (Rust ↔ Lean) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-kernel --bin axiograph_revision_digest -- ../examples/economics/EconomicFlows.axi > ../$(BUILD_DIR)/revision_digest_rust_economic.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean --revision-digest-v2 ../examples/economics/EconomicFlows.axi > ../$(BUILD_DIR)/revision_digest_lean_economic.txt ) && \
		diff -u $(BUILD_DIR)/revision_digest_rust_economic.txt $(BUILD_DIR)/revision_digest_lean_economic.txt && \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-kernel --bin axiograph_revision_digest -- ../examples/learning/MachinistLearning.axi > ../$(BUILD_DIR)/revision_digest_rust_learning.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean --revision-digest-v2 ../examples/learning/MachinistLearning.axi > ../$(BUILD_DIR)/revision_digest_lean_learning.txt ) && \
		diff -u $(BUILD_DIR)/revision_digest_rust_learning.txt $(BUILD_DIR)/revision_digest_lean_learning.txt && \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-kernel --bin axiograph_revision_digest -- ../examples/ontology/SchemaEvolution.axi > ../$(BUILD_DIR)/revision_digest_rust_ontology.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean --revision-digest-v2 ../examples/ontology/SchemaEvolution.axi > ../$(BUILD_DIR)/revision_digest_lean_ontology.txt ) && \
		diff -u $(BUILD_DIR)/revision_digest_rust_ontology.txt $(BUILD_DIR)/revision_digest_lean_ontology.txt && \
		echo "✓ Rust and Lean revision identities agree on exact accepted bytes"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run identity parity"; \
	fi

verify-lean-e2e-query-result-module-v4: dirs
	@echo "━━━ Rust → Lean prepared-query and answer binding (query_result_v4) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_verify ) && \
		( cd $(RUST_DIR) && $(CARGO) test -q -p axiograph-cli approved_lean_checker_matches_prepared_ast_goldens -- --nocapture ) && \
		echo "✓ Rust and Lean agree on query_result_v4 prepared/answer bindings"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-axi-well-typed-v1: dirs
	@echo "━━━ Rust → Lean certificate check (axi_well_typed_v1) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert typecheck ../examples/economics/EconomicFlows.axi --out ../$(BUILD_DIR)/axi_well_typed_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/economics/EconomicFlows.axi ../$(BUILD_DIR)/axi_well_typed_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (axi_well_typed_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-axi-constraints-ok-v1: dirs
	@echo "━━━ Rust → Lean certificate check (axi_constraints_ok_v1) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert constraints ../examples/runtime_theory/ConstraintsOkDemo.axi --out ../$(BUILD_DIR)/axi_constraints_ok_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/runtime_theory/ConstraintsOkDemo.axi ../$(BUILD_DIR)/axi_constraints_ok_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (axi_constraints_ok_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-resolution-v2: dirs
	@echo "━━━ Rust → Lean certificate check (resolution v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_resolution_cert_v2 > ../$(BUILD_DIR)/resolution_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/resolution_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (resolution v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-normalize-path-v2: dirs
	@echo "━━━ Rust → Lean certificate check (normalize_path v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_normalize_path_cert_v2 > ../$(BUILD_DIR)/normalize_path_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_verify && $(LEAN_ENV) .lake/build/bin/axiograph_verify ../$(BUILD_DIR)/normalize_path_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (normalize_path v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-rewrite-derivation-v3: dirs
	@echo "━━━ Rust → Lean certificate check (rewrite_derivation v3, .axi rules) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_rewrite_derivation_cert_v3 -- ../fixtures/verification/rewrite_rules_anchor_v1.axi > ../$(BUILD_DIR)/rewrite_derivation_from_rust_v3.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../fixtures/verification/rewrite_rules_anchor_v1.axi ../$(BUILD_DIR)/rewrite_derivation_from_rust_v3.json ) && \
		echo "✓ Rust → Lean certificate verified (rewrite_derivation v3)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-ontology-rewrites-v3: dirs
	@echo "━━━ Rust → Lean certificate check (rewrite_derivation v3, domain .axi rules) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_ontology_rewrite_derivation_cert_v3 -- ../examples/ontology/OntologyRewrites.axi > ../$(BUILD_DIR)/ontology_rewrite_derivation_from_rust_v3.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/ontology/OntologyRewrites.axi ../$(BUILD_DIR)/ontology_rewrite_derivation_from_rust_v3.json ) && \
		echo "✓ Rust → Lean certificate verified (ontology rewrite_derivation v3)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-path-equiv-v2: dirs
	@echo "━━━ Rust → Lean certificate check (path_equiv v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_path_equiv_cert_v2 > ../$(BUILD_DIR)/path_equiv_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_verify && $(LEAN_ENV) .lake/build/bin/axiograph_verify ../$(BUILD_DIR)/path_equiv_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (path_equiv v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-path-equiv-congr-v2: dirs
	@echo "━━━ Rust → Lean certificate check (path_equiv congruence v2) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_path_equiv_congr_cert_v2 > ../$(BUILD_DIR)/path_equiv_congr_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build axiograph_verify && $(LEAN_ENV) .lake/build/bin/axiograph_verify ../$(BUILD_DIR)/path_equiv_congr_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (path_equiv congruence v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-indexed-path-theory: verify-lean-e2e-normalize-path-v2 verify-lean-e2e-path-equiv-v2 verify-lean-e2e-path-equiv-congr-v2
	@echo "━━━ Verifying endpoint-indexed path laws and fail-closed traces ━━━"
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb --lib normalize_path_v2_tests --locked
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb --test path_expr_property_tests --locked
	python3 scripts/check_indexed_path_certificate_rejections.py
	@echo "✓ Indexed path certificates have Rust/Lean parity and reject malformed traces"

verify-lean-e2e-delta-f-v1: dirs
	@echo "━━━ Rust → Lean certificate check (delta_f v1) ━━━"
	@ if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_delta_f_cert_v1 > ../$(BUILD_DIR)/delta_f_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/delta_f_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (delta_f v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-suite: verify-lean-e2e-category-kernel-v3 verify-lean-e2e-axi-well-typed-v1 verify-lean-e2e-axi-constraints-ok-v1 verify-lean-e2e-query-result-module-v4 verify-lean-e2e-resolution-v2 verify-lean-indexed-path-theory verify-lean-e2e-rewrite-derivation-v3 verify-lean-e2e-ontology-rewrites-v3 verify-lean-e2e-delta-f-v1

# ============================================================================
# Binaries
# ============================================================================

binaries: rust
	@echo "━━━ Creating binaries ━━━"
	cp $(RUST_DIR)/target/release/axiograph $(BIN_DIR)/axiograph
	@echo "✓ Binaries installed to $(BIN_DIR)/"

install: binaries
	@echo "━━━ Installing to /usr/local/bin ━━━"
	@sudo cp $(BIN_DIR)/axiograph /usr/local/bin/axiograph
	@echo "✓ Installed: axiograph"

# ============================================================================
# Demo
# ============================================================================

demo: all
	@echo ""
	@echo "━━━ Running End-to-End Demo ━━━"
	@echo ""
	cd $(EXAMPLES_DIR) && ./run_demo.sh

demo-quick: rust
	@echo "━━━ Quick Demo (Rust only) ━━━"
	cd $(RUST_DIR) && $(CARGO) run --release --example machining_demo

# ============================================================================
# Tests
# ============================================================================

test: check-no-unsafe rust-test verify-semantics
	@echo ""
	@echo "━━━ All Tests Complete ━━━"

verify-semantics: check-no-unsafe rust-test-semantics verify-lean-theory verify-lean-certificates verify-lean-certificate-rejections verify-lean-e2e-suite verify-lean-semantic-vcs verify-axi-store verify-axi-parse-e2e verify-identity-parity verify-regulated-shipment
	@echo ""
	@echo "━━━ Semantics Verification Complete ━━━"

verify-release-fixtures: lean-exe
	@echo "━━━ Running hash-pinned packaged-checker accept/reject corpus ━━━"
	python3 scripts/run_release_fixture_suite.py \
		--checker $(LEAN_DIR)/.lake/build/bin/axiograph_verify
	@echo "✓ Packaged checker accepted one exact finite result and rejected every pinned adversary"

verify-release-packaging:
	@echo "━━━ Testing deterministic manifests, archives, corruption rejection, and publication ordering ━━━"
	python3 -m unittest discover -s scripts/tests -p 'test_*.py' -v
	@echo "✓ Release manifests, bundles, corruption checks, and local publication rehearsal passed"

rehearse-release-publication:
	@echo "━━━ Rehearsing fail-before-publish ordering locally ━━━"
	python3 scripts/rehearse_release_publication.py
	@echo "✓ Every injected failure and corrupted asset remained unpublished; one audited set committed atomically"

release-gate: check-rust-toolchain check-clean-source-manifest check-example-catalog check-greenfield-surface rust-fmt-check check-no-unsafe rust-test-all-targets-features check-cli-feature-matrix verify-release-packaging verify-release-fixtures verify-semantics
	python3 scripts/generate_release_source_manifest.py --check-only >/dev/null
	git diff --check
	@echo ""
	@echo "━━━ Exact Clean-Checkout Release Gate Complete ━━━"

test-semantics: verify-semantics

test-e2e: all
	@echo "━━━ Running E2E Tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test --release --test integration_tests
	@echo "✓ E2E tests complete"

test-property:
	@echo "━━━ Running Property Tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test --release -p axiograph-llm-sync --test property_tests

test-backend-containers:
	@echo "━━━ Running backend container readback tests (TypeDB + TerminusDB) ━━━"
	cd $(RUST_DIR) && AXIOGRAPH_RUN_BACKEND_CONTAINER_TESTS=1 $(CARGO) test --test backend_container_tests -- --ignored --nocapture

# ============================================================================
# Formal Verification (Verus, optional)
# ============================================================================

verify-verus:
	@echo "━━━ Verifying Verus crate (optional) ━━━"
	@ if command -v verus >/dev/null 2>&1; then \
		cd $(RUST_DIR)/verus && verus src/lib.rs && echo "✓ Verus verification complete"; \
	else \
		echo "⚠️  verus not found - skipping (install: https://github.com/verus-lang/verus)"; \
	fi

# ============================================================================
# Documentation
# ============================================================================

docs: rust
	@echo "━━━ Building Documentation ━━━"
	cd $(RUST_DIR) && $(CARGO) doc --no-deps --all-features
	@echo "✓ Docs available at $(RUST_DIR)/target/doc/index.html"

# ============================================================================
# Frontend (Viz)
# ============================================================================

viz-install:
	@echo "━━━ Installing viz frontend deps ━━━"
	cd frontend/viz && npm install
	@echo "✓ Viz deps installed"

viz-build:
	@echo "━━━ Building viz frontend ━━━"
	cd frontend/viz && npm install && npm run build
	@echo "✓ Viz frontend built (frontend/viz/dist)"

viz-build-debug:
	@echo "━━━ Building viz frontend (debug) ━━━"
	cd frontend/viz && npm install && npm run build:debug
	@echo "✓ Viz frontend built (debug) (frontend/viz/dist)"

viz-dev:
	@echo "━━━ Starting viz frontend dev server ━━━"
	@echo "Tip: open the Vite dev URL and point Axiograph to it for UI iteration."
	cd frontend/viz && npm install && npm run dev

# ============================================================================
# Clean
# ============================================================================

clean:
	@echo "━━━ Cleaning build artifacts ━━━"
	rm -rf $(BUILD_DIR)
	rm -rf $(BIN_DIR)
	cd $(RUST_DIR) && $(CARGO) clean
	rm -rf $(LEAN_DIR)/.lake
	rm -rf $(LEAN_DIR)/build
	@echo "✓ Clean complete"

# ============================================================================
# Help
# ============================================================================

help:
	@echo "Axiograph Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all          Build everything (Rust + Lean)"
	@echo "  rust         Build Rust crates only"
	@echo "  lean         Build Lean checker"
	@echo "  demo         Run full end-to-end demo"
	@echo "  demo-quick   Run Rust-only demo"
	@echo "  test         Run all tests"
	@echo "  release-gate Run the clean-checkout Rust + Lean publication decision"
	@echo "  test-e2e     Run end-to-end tests"
	@echo "  test-backend-containers  Run Docker-backed TypeDB / TerminusDB readback tests"
	@echo "  docs         Build documentation"
	@echo "  install      Install binaries to /usr/local/bin"
	@echo "  clean        Remove build artifacts"
	@echo ""
	@echo "Development:"
	@echo "  rust-debug   Build Rust in debug mode"
	@echo "  lean         Build Lean checker"
	@echo "  lean-update  Explicitly refresh the pinned Lake manifest"
	@echo "  lean-system-cc  Build Lean with SDKROOT (macOS)"
	@echo "  lean-exe     Build axiograph_verify executable"
	@echo "  verify-lean-cert  Verify CERT=... (optional AXI=...)"
	@echo "  verify-lean-e2e-suite  Rust → Lean canonical certificate checks"
	@echo "  verify-lean-theory  Check finite category/dependent/groupoid semantics and adversarial cases"
	@echo "  verify-lean-semantic-vcs  Verify Rust merge/rebase plans against Lean theory"
	@echo "  check-no-unsafe  Reject unsafe code in all first-party Rust targets"
	@echo "  verify-release-fixtures  Run the hash-pinned Lean accept/reject release corpus"
	@echo "  verify-release-packaging  Test manifests, reproducible bundles, corruption, and publication ordering"
	@echo "  rehearse-release-publication  Run local fail-before-publish failure injection"
	@echo "  verify-canonical-spine  Focused canonical spine gate across Rust, examples, and Lean"
	@echo "  verify-semantics  Rust+Lean identity, certificate, lineage, merge, and storage suite"
	@echo "  viz-build    Build the viz frontend (frontend/viz/dist)"
	@echo "  viz-build-debug  Build the viz frontend without minify + with sourcemaps"
	@echo "  viz-dev      Run the viz frontend dev server (Vite)"
	@echo ""
	@echo "Prerequisites:"
	@echo "  - Rust $(RUST_VERSION) exactly for release-gate"
	@echo "  - Lean4 + Lake (optional for verification)"
