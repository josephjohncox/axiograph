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

.PHONY: all all-exe rust lean lean-cache lean-system-cc lean-exe verify-lean verify-lean-cert verify-lean-e2e verify-lean-v2 verify-lean-e2e-v2 \
	verify-lean-axi-schema-v1 \
	verify-lean-axi-v1 \
	verify-axi-parse-e2e \
	verify-pathdb-export-axi-v1 \
	verify-verus \
	verify-lean-resolution-v2 verify-lean-normalize-path-v2 verify-lean-path-equiv-v2 verify-lean-delta-f-v1 \
	verify-lean-e2e-v2-anchored \
	verify-lean-e2e-axi-well-typed-v1 \
	verify-lean-e2e-axi-constraints-ok-v1 \
	verify-lean-e2e-query-result-v1 \
	verify-lean-e2e-query-result-v2 \
	verify-lean-e2e-query-result-module-v3 \
	verify-lean-e2e-resolution-v2 verify-lean-e2e-normalize-path-v2 verify-lean-e2e-path-equiv-v2 verify-lean-e2e-path-equiv-congr-v2 verify-lean-e2e-delta-f-v1 \
	verify-lean-certificates verify-lean-e2e-suite \
	rust-test-semantics verify-semantics test-semantics \
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

# Detect OS
UNAME := $(shell uname)
ifeq ($(UNAME), Darwin)
    DYLIB_EXT := dylib
    SHARED_FLAG := -dynamiclib
else
    DYLIB_EXT := so
    SHARED_FLAG := -shared
endif

# Rust configuration
CARGO := cargo
CARGO_OPTS := --release
CARGO_FEATURES ?=

# Lean configuration (optional)
LAKE := lake

# On macOS, Lean's bundled clang needs a valid macOS SDK to link executables.
# We keep Lean's toolchain (so it can find its bundled libs like `libgmp.a`),
# but we provide `SDKROOT` via `xcrun` to point it at the system SDK.
ifeq ($(UNAME), Darwin)
MACOSX_DEPLOYMENT_TARGET ?= 13.0
LEAN_ENV := SDKROOT="$$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)" MACOSX_DEPLOYMENT_TARGET="$(MACOSX_DEPLOYMENT_TARGET)"
else
LEAN_ENV :=
endif

# ============================================================================
# Default target
# ============================================================================

all: dirs rust lean binaries
	@echo ""
	@echo "╔══════════════════════════════════════════════════════════════╗"
	@echo "║              AXIOGRAPH BUILD COMPLETE                        ║"
	@echo "╚══════════════════════════════════════════════════════════════╝"
	@echo ""
	@echo "Binaries available in $(BIN_DIR)/"
	@echo "  - axiograph          : Main CLI tool"
	@echo "  - axiograph-cli      : Compatibility alias (same binary)"
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

rust-test-semantics:
	@echo "━━━ Running Rust semantics tests (axiograph-pathdb) ━━━"
	cd $(RUST_DIR) && $(CARGO) test -p axiograph-pathdb
	@echo "✓ Rust semantics tests complete"

# ============================================================================
# Lean Build (additive, optional)
# ============================================================================

lean-cache: dirs
	@echo "━━━ Updating Lean dependencies/cache ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		(cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) update) || echo "⚠️  lake update failed (offline?); continuing"; \
		(cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe cache get) || echo "⚠️  lake exe cache get failed (offline?); continuing"; \
		echo "✓ Lean cache step complete"; \
	else \
		echo "⚠️  lake (Lean) not found - skipping Lean cache update"; \
		echo "   Install via elan: https://leanprover-community.github.io/get_started.html"; \
	fi

lean: dirs lean-cache
	@echo "━━━ Building Lean checker ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && echo "✓ Lean build complete"; \
	else \
		echo "⚠️  lake (Lean) not found - skipping Lean build"; \
		echo "   Install via elan: https://leanprover-community.github.io/get_started.html"; \
	fi

lean-system-cc: lean

lean-exe: dirs lean-cache
	@echo "━━━ Building Lean checker executable (axiograph_verify) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
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
		echo "⚠️  lake (Lean) not found - skipping Lean exe build"; \
		echo "   Install via elan: https://leanprover-community.github.io/get_started.html"; \
	fi

verify-lean: lean
	@echo "━━━ Running Lean checker (scaffold) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/certificates/reachability_v1.json && echo "✓ Lean checker ran"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-cert: lean-exe
	@echo "━━━ Running Lean checker executable (custom cert) ━━━"
	@if [ -z "$(CERT)" ]; then \
		echo "error: set CERT=/path/to/certificate.json (and optional AXI=/path/to/anchor.axi)"; \
		exit 2; \
	fi
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		if [ -n "$(AXI)" ]; then \
			cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_verify "$(abspath $(AXI))" "$(abspath $(CERT))" && echo "✓ Lean verified cert: $(CERT)"; \
		else \
			cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) exe axiograph_verify "$(abspath $(CERT))" && echo "✓ Lean verified cert: $(CERT)"; \
		fi; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-v2: lean
	@echo "━━━ Running Lean checker (fixed-point cert v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/certificates/reachability_v2.json && echo "✓ Lean checker ran (v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-resolution-v2: lean
	@echo "━━━ Running Lean checker (resolution cert v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/certificates/resolution_v2.json && echo "✓ Lean checker ran (resolution v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-normalize-path-v2: lean
	@echo "━━━ Running Lean checker (normalize_path cert v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/certificates/normalize_path_v2.json && echo "✓ Lean checker ran (normalize_path v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-path-equiv-v2: lean
	@echo "━━━ Running Lean checker (path_equiv cert v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/certificates/path_equiv_v2.json && echo "✓ Lean checker ran (path_equiv v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-delta-f-v1: lean
	@echo "━━━ Running Lean checker (delta_f cert v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/certificates/delta_f_v1.json && echo "✓ Lean checker ran (delta_f v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-certificates: lean
	@echo "━━━ Running Lean checker (certificate fixtures) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/anchors/*.axi ../examples/certificates/*.json && echo "✓ Lean verified certificate fixtures"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-axi-schema-v1: lean
	@echo "━━━ Parsing canonical schema .axi corpus (Lean) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/SchemaV1ParseMain.lean ../examples/economics/EconomicFlows.axi ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/SchemaV1ParseMain.lean ../examples/ontology/SchemaEvolution.axi ) && \
		echo "✓ Lean parsed canonical schema corpus"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run parser"; \
	fi

verify-lean-axi-v1: lean
	@echo "━━━ Parsing canonical .axi corpus (Lean, axi_v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/economics/EconomicFlows.axi ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/learning/MachinistLearning.axi ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../examples/ontology/SchemaEvolution.axi ) && \
		echo "✓ Lean parsed canonical corpus (axi_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run parser"; \
	fi

verify-axi-parse-e2e: lean
	@echo "━━━ Parsing canonical .axi corpus (Rust ↔ Lean, axi_v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
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

verify-pathdb-export-axi-v1: lean dirs
	@echo "━━━ Parsing PathDB export snapshot (Rust ↔ Lean, axi_v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-pathdb --example emit_pathdb_export_axi_v1 > ../$(BUILD_DIR)/pathdb_export_snapshot_v1.axi ) && \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-dsl --bin axiograph_parse_axi_v1 -- ../$(BUILD_DIR)/pathdb_export_snapshot_v1.axi > ../$(BUILD_DIR)/axi_v1_rust_pathdb_export.txt ) && \
		( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/Axi/AxiV1ParseMain.lean ../$(BUILD_DIR)/pathdb_export_snapshot_v1.axi > ../$(BUILD_DIR)/axi_v1_lean_pathdb_export.txt ) && \
		diff -u $(BUILD_DIR)/axi_v1_rust_pathdb_export.txt $(BUILD_DIR)/axi_v1_lean_pathdb_export.txt && \
		echo "✓ Rust and Lean parsers agree on PathDB export snapshot (PathDBExportV1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run parse e2e"; \
	fi

verify-lean-e2e: dirs
	@echo "━━━ Rust → Lean certificate check ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_reachability_cert > ../$(BUILD_DIR)/reachability_from_rust.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/reachability_from_rust.json ) && \
		echo "✓ Rust → Lean certificate verified"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-v2: dirs
	@echo "━━━ Rust → Lean certificate check (v2 fixed-point) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_reachability_cert_v2 > ../$(BUILD_DIR)/reachability_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/reachability_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-v2-anchored: dirs
	@echo "━━━ Rust → Lean certificate check (v2 anchored to .axi) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_reachability_cert_v2_anchored -- ../$(BUILD_DIR)/reachability_anchor_v1.axi > ../$(BUILD_DIR)/reachability_from_rust_v2_anchored.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/reachability_anchor_v1.axi ../$(BUILD_DIR)/reachability_from_rust_v2_anchored.json ) && \
		echo "✓ Rust → Lean certificate verified (v2 anchored)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-query-result-v1: dirs
	@echo "━━━ Rust → Lean certificate check (query_result_v1 anchored) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert query ../examples/anchors/pathdb_export_anchor_v1.axi --lang axql 'select ?y where 0 -r1/r2-> ?y' > ../$(BUILD_DIR)/query_result_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/anchors/pathdb_export_anchor_v1.axi ../$(BUILD_DIR)/query_result_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (query_result_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-query-result-v2: dirs
	@echo "━━━ Rust → Lean certificate check (query_result_v2 / disjunction anchored) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert query ../examples/anchors/pathdb_export_anchor_v1.axi --lang axql 'select ?y where 0 -r1-> ?y or 0 -r1/r2-> ?y' > ../$(BUILD_DIR)/query_result_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/anchors/pathdb_export_anchor_v1.axi ../$(BUILD_DIR)/query_result_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (query_result_v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-query-result-module-v3: dirs
	@echo "━━━ Rust → Lean certificate check (query_result_v3 anchored to canonical .axi) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert query ../examples/manufacturing/SupplyChainHoTT.axi --lang axql 'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' --anchor-out ../$(BUILD_DIR)/supply_chain_hott_anchor_export_v1.axi > ../$(BUILD_DIR)/query_result_from_module_v3.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/manufacturing/SupplyChainHoTT.axi ../$(BUILD_DIR)/query_result_from_module_v3.json ) && \
		echo "✓ Rust → Lean certificate verified (query_result_v3 from module)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

# Back-compat alias (the cert is now `.axi`-anchored, so it's v3).
verify-lean-e2e-query-result-module-v1: verify-lean-e2e-query-result-module-v3

verify-lean-e2e-axi-well-typed-v1: dirs
	@echo "━━━ Rust → Lean certificate check (axi_well_typed_v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert typecheck ../examples/economics/EconomicFlows.axi --out ../$(BUILD_DIR)/axi_well_typed_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/economics/EconomicFlows.axi ../$(BUILD_DIR)/axi_well_typed_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (axi_well_typed_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-axi-constraints-ok-v1: dirs
	@echo "━━━ Rust → Lean certificate check (axi_constraints_ok_v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -q -p axiograph-cli -- cert constraints ../examples/demo_data/ConstraintsOkDemo.axi --out ../$(BUILD_DIR)/axi_constraints_ok_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/demo_data/ConstraintsOkDemo.axi ../$(BUILD_DIR)/axi_constraints_ok_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (axi_constraints_ok_v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-resolution-v2: dirs
	@echo "━━━ Rust → Lean certificate check (resolution v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_resolution_cert_v2 > ../$(BUILD_DIR)/resolution_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/resolution_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (resolution v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-normalize-path-v2: dirs
	@echo "━━━ Rust → Lean certificate check (normalize_path v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_normalize_path_cert_v2 > ../$(BUILD_DIR)/normalize_path_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/normalize_path_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (normalize_path v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-rewrite-derivation-v3: dirs
	@echo "━━━ Rust → Lean certificate check (rewrite_derivation v3, .axi rules) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_rewrite_derivation_cert_v3 -- ../examples/anchors/rewrite_rules_anchor_v1.axi > ../$(BUILD_DIR)/rewrite_derivation_from_rust_v3.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/anchors/rewrite_rules_anchor_v1.axi ../$(BUILD_DIR)/rewrite_derivation_from_rust_v3.json ) && \
		echo "✓ Rust → Lean certificate verified (rewrite_derivation v3)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-ontology-rewrites-v3: dirs
	@echo "━━━ Rust → Lean certificate check (rewrite_derivation v3, domain .axi rules) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_ontology_rewrite_derivation_cert_v3 -- ../examples/ontology/OntologyRewrites.axi > ../$(BUILD_DIR)/ontology_rewrite_derivation_from_rust_v3.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../examples/ontology/OntologyRewrites.axi ../$(BUILD_DIR)/ontology_rewrite_derivation_from_rust_v3.json ) && \
		echo "✓ Rust → Lean certificate verified (ontology rewrite_derivation v3)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-path-equiv-v2: dirs
	@echo "━━━ Rust → Lean certificate check (path_equiv v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_path_equiv_cert_v2 > ../$(BUILD_DIR)/path_equiv_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/path_equiv_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (path_equiv v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-path-equiv-congr-v2: dirs
	@echo "━━━ Rust → Lean certificate check (path_equiv congruence v2) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_path_equiv_congr_cert_v2 > ../$(BUILD_DIR)/path_equiv_congr_from_rust_v2.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/path_equiv_congr_from_rust_v2.json ) && \
		echo "✓ Rust → Lean certificate verified (path_equiv congruence v2)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-delta-f-v1: dirs
	@echo "━━━ Rust → Lean certificate check (delta_f v1) ━━━"
	@if command -v $(LAKE) >/dev/null 2>&1; then \
		( cd $(RUST_DIR) && $(CARGO) run -p axiograph-pathdb --example emit_delta_f_cert_v1 > ../$(BUILD_DIR)/delta_f_from_rust_v1.json ) && \
			( cd $(LEAN_DIR) && $(LEAN_ENV) $(LAKE) build Axiograph && $(LEAN_ENV) $(LAKE) env lean --run Axiograph/VerifyMain.lean ../$(BUILD_DIR)/delta_f_from_rust_v1.json ) && \
		echo "✓ Rust → Lean certificate verified (delta_f v1)"; \
	else \
		echo "⚠️  lake (Lean) not found - cannot run checker"; \
	fi

verify-lean-e2e-suite: verify-lean-e2e verify-lean-e2e-v2 verify-lean-e2e-v2-anchored verify-lean-e2e-axi-well-typed-v1 verify-lean-e2e-axi-constraints-ok-v1 verify-lean-e2e-query-result-v1 verify-lean-e2e-query-result-v2 verify-lean-e2e-query-result-module-v3 verify-lean-e2e-resolution-v2 verify-lean-e2e-normalize-path-v2 verify-lean-e2e-rewrite-derivation-v3 verify-lean-e2e-ontology-rewrites-v3 verify-lean-e2e-path-equiv-v2 verify-lean-e2e-path-equiv-congr-v2 verify-lean-e2e-delta-f-v1

# ============================================================================
# Binaries
# ============================================================================

binaries: rust
	@echo "━━━ Creating binaries ━━━"
	cp $(RUST_DIR)/target/release/axiograph $(BIN_DIR)/axiograph
	cp $(RUST_DIR)/target/release/axiograph $(BIN_DIR)/axiograph-cli
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

test: rust-test verify-semantics
	@echo ""
	@echo "━━━ All Tests Complete ━━━"

verify-semantics: rust-test-semantics verify-lean-certificates verify-lean-e2e-suite verify-axi-parse-e2e verify-pathdb-export-axi-v1
	@echo ""
	@echo "━━━ Semantics Verification Complete ━━━"

test-semantics: verify-semantics

test-e2e: all
	@echo "━━━ Running E2E Tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test --release --test integration_tests
	@echo "✓ E2E tests complete"

test-property:
	@echo "━━━ Running Property Tests ━━━"
	cd $(RUST_DIR) && $(CARGO) test --release -p axiograph-llm-sync --test property_tests

# ============================================================================
# Formal Verification (Verus, optional)
# ============================================================================

verify-verus:
	@echo "━━━ Verifying Verus crate (optional) ━━━"
	@if command -v verus >/dev/null 2>&1; then \
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
	@echo "  test-e2e     Run end-to-end tests"
	@echo "  docs         Build documentation"
	@echo "  install      Install binaries to /usr/local/bin"
	@echo "  clean        Remove build artifacts"
	@echo ""
	@echo "Development:"
	@echo "  rust-debug   Build Rust in debug mode"
	@echo "  lean         Build Lean checker"
	@echo "  lean-system-cc  Build Lean with SDKROOT (macOS)"
	@echo "  lean-exe     Build axiograph_verify executable"
	@echo "  verify-lean  Run Lean checker (optional)"
	@echo "  verify-lean-cert  Verify CERT=... (optional AXI=...)"
	@echo "  verify-lean-e2e  Rust → Lean certificate check"
	@echo "  verify-semantics  Focused Rust+Lean semantics suite"
	@echo "  viz-build    Build the viz frontend (frontend/viz/dist)"
	@echo "  viz-build-debug  Build the viz frontend without minify + with sourcemaps"
	@echo "  viz-dev      Run the viz frontend dev server (Vite)"
	@echo ""
	@echo "Prerequisites:"
	@echo "  - Rust 1.75+ (cargo)"
	@echo "  - Lean4 + Lake (optional for verification)"
