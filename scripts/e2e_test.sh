#!/bin/bash
# ============================================================================
# Axiograph Verification Driver
# ============================================================================
#
# This script runs the current verification flow for the Axiograph workspace:
# 1. Rust build + unit/integration tests
# 2. Lean checker + certificate verification (when `lake` is installed)
# 3. Canonical semantic-spine checks and explicit debug/parser-parity gates
#
# Usage:
#   ./scripts/e2e_test.sh           # Run all tests
#   ./scripts/e2e_test.sh --quick   # Run quick tests only
#   ./scripts/e2e_test.sh --rust    # Rust only
#   ./scripts/e2e_test.sh --lean    # Lean-only verification suite (requires `lake`)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0
RUN=0
OUTPUT_FILE="$(mktemp -t axiograph-test.XXXXXX)"
trap 'rm -f "$OUTPUT_FILE"' EXIT

# Parse args
QUICK=false
RUST_ONLY=false
LEAN_ONLY=false

for arg in "$@"; do
	case $arg in
	--quick)
		QUICK=true
		;;
	--rust)
		RUST_ONLY=true
		;;
	--lean)
		LEAN_ONLY=true
		;;
	--help | -h)
		sed -n '2,14p' "$0"
		exit 0
		;;
	*)
		echo "error: unknown argument: $arg" >&2
		exit 2
		;;
	esac
done

if [ "$RUST_ONLY" = true ] && [ "$LEAN_ONLY" = true ]; then
	echo "error: --rust and --lean are mutually exclusive" >&2
	exit 2
fi

# Helper functions
print_header() {
	echo ""
	echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
	echo -e "${BLUE}║ $1${NC}"
	echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
	echo ""
}

print_test() {
	echo -e "  ${YELLOW}▶${NC} $1"
}

print_pass() {
	echo -e "  ${GREEN}✓${NC} $1"
	PASSED=$((PASSED + 1))
}

print_fail() {
	echo -e "  ${RED}✗${NC} $1"
	FAILED=$((FAILED + 1))
}

print_skip() {
	echo -e "  ${YELLOW}⊘${NC} $1 (skipped)"
	SKIPPED=$((SKIPPED + 1))
}

run_test() {
	local name="$1"
	local cmd="$2"

	RUN=$((RUN + 1))
	print_test "$name"

	if eval "$cmd" >"$OUTPUT_FILE" 2>&1; then
		print_pass "$name"
	else
		print_fail "$name"
		echo "    Output:"
		head -20 "$OUTPUT_FILE" | sed 's/^/    /'
	fi
}

# ============================================================================
# Main
# ============================================================================

echo ""
echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║           AXIOGRAPH VERIFICATION DRIVER                       ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Project root: $PROJECT_ROOT"
echo "Quick mode: $QUICK"
echo ""

# ============================================================================
# Step 1: Rust Build (via Makefile)
# ============================================================================

if [ "$LEAN_ONLY" = false ]; then
	print_header "Step 1: Rust Build"
	cd "$PROJECT_ROOT"

	if [ "$QUICK" = true ]; then
		run_test "Rust build (debug)" "make rust-debug"
	else
		run_test "Rust build (release)" "make rust"
		run_test "Install CLI binaries" "make binaries"
	fi
fi

# ============================================================================
# Step 2: Rust Tests
# ============================================================================

if [ "$LEAN_ONLY" = false ]; then
	print_header "Step 2: Rust Tests"
	cd "$PROJECT_ROOT"

	if [ "$QUICK" = true ]; then
		run_test "Rust semantics tests (axiograph-pathdb)" "make rust-test-semantics" || true
	else
		run_test "Rust tests (workspace)" "make rust-test" || true
	fi
fi

# ============================================================================
# Step 3: Lean + Semantics Verification
# ============================================================================

if [ "$RUST_ONLY" = false ]; then
	print_header "Step 3: Lean + Semantics Verification"
	cd "$PROJECT_ROOT"

	if command -v lake &>/dev/null; then
		if [ "$QUICK" = true ]; then
			run_test "Lean certificate fixtures" "make verify-lean-certificates" || true
		else
			run_test "Semantics suite (Rust+Lean)" "make verify-semantics" || true
		fi
	else
		print_skip "Lean verification (lake not installed)"
	fi
fi

# ============================================================================
# Step 4: E2E Demo (optional)
# ============================================================================

if [ "$LEAN_ONLY" = false ] && [ "$QUICK" = false ]; then
	print_header "Step 4: End-to-End Demo"
	cd "$PROJECT_ROOT"
	run_test "E2E demo (examples/run_demo.sh)" "make demo" || true
fi

# ============================================================================
# ============================================================================
# Step 5: Focused Property/Regression Tests
# ============================================================================

if [ "$LEAN_ONLY" = false ] && [ "$QUICK" = false ]; then
	print_header "Step 5: Focused Property/Regression Tests"

	cd "$PROJECT_ROOT"

	run_test "Reconciliation property tests" \
		"cargo test --manifest-path rust/Cargo.toml -p axiograph-llm-sync --test reconciliation_tests --no-fail-fast" || true

	run_test "Path verification property tests" \
		"cargo test --manifest-path rust/Cargo.toml -p axiograph-llm-sync --test path_verification_tests --no-fail-fast" || true
fi

# ============================================================================
# Step 6: Code Quality
# ============================================================================

if [ "$LEAN_ONLY" = false ]; then
	print_header "Step 6: Code Quality"

	cd "$PROJECT_ROOT"

	run_test "Rust fmt check" "cargo fmt --manifest-path rust/Cargo.toml --all --check" || true
	run_test "Rust clippy" "cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings" || true
fi

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                      TEST SUMMARY                            ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
echo ""

if [ "$RUN" -eq 0 ]; then
	echo -e "${RED}No tests ran; refusing a vacuous success.${NC}"
	exit 1
elif [ "$FAILED" -gt 0 ]; then
	echo -e "${RED}Some tests failed!${NC}"
	exit 1
else
	echo -e "${GREEN}All tests passed!${NC}"
	exit 0
fi
