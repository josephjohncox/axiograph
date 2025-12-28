#!/bin/bash
# ============================================================================
# Axiograph End-to-End Integration Tests
# ============================================================================
#
# This script runs comprehensive E2E tests for the entire Axiograph system:
# 1. Rust build + unit/integration tests
# 2. Lean checker + certificate verification (when `lake` is installed)
# 3. Rust↔Lean parser parity and core end-to-end semantics checks
#
# Usage:
#   ./scripts/e2e_test.sh           # Run all tests
#   ./scripts/e2e_test.sh --quick   # Run quick tests only
#   ./scripts/e2e_test.sh --rust    # Rust only
#   ./scripts/e2e_test.sh --lean    # Lean-only verification suite (requires `lake`)

set -e

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
    esac
done

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
    ((PASSED++))
}

print_fail() {
    echo -e "  ${RED}✗${NC} $1"
    ((FAILED++))
}

print_skip() {
    echo -e "  ${YELLOW}⊘${NC} $1 (skipped)"
    ((SKIPPED++))
}

run_test() {
    local name="$1"
    local cmd="$2"
    
    print_test "$name"
    
    if eval "$cmd" > /tmp/axiograph_test_output.txt 2>&1; then
        print_pass "$name"
        return 0
    else
        print_fail "$name"
        echo "    Output:"
        head -20 /tmp/axiograph_test_output.txt | sed 's/^/    /'
        return 1
    fi
}

# ============================================================================
# Main
# ============================================================================

echo ""
echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║           AXIOGRAPH END-TO-END TEST SUITE                    ║${NC}"
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

    if command -v lake &> /dev/null; then
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
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║                        SUMMARY                               ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "Passed:  ${GREEN}${PASSED}${NC}"
echo -e "Failed:  ${RED}${FAILED}${NC}"
echo -e "Skipped: ${YELLOW}${SKIPPED}${NC}"
echo ""

if [ "$FAILED" -ne 0 ]; then
    exit 1
fi

# ============================================================================
# Step 7: Property Tests
# ============================================================================

if [ "$IDRIS_ONLY" = false ] && [ "$QUICK" = false ]; then
    print_header "Step 7: Property-Based Tests"
    
    cd "$PROJECT_ROOT/rust"
    
    run_test "Reconciliation property tests" \
        "cargo test -p axiograph-llm-sync --test reconciliation_tests --no-fail-fast" || true
    
    run_test "Path verification property tests" \
        "cargo test -p axiograph-llm-sync --test path_verification_tests --no-fail-fast" || true
fi

# ============================================================================
# Step 8: Format Check
# ============================================================================

if [ "$IDRIS_ONLY" = false ]; then
    print_header "Step 8: Code Quality"
    
    cd "$PROJECT_ROOT/rust"
    
    run_test "Rust fmt check" "cargo fmt --check" || true
    run_test "Rust clippy" "cargo clippy --all-targets -- -D warnings" || true
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

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
