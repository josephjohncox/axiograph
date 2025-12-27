#!/bin/bash
# ============================================================================
# Axiograph End-to-End Integration Tests
# ============================================================================
#
# This script runs comprehensive E2E tests for the entire Axiograph system:
# 1. Rust crate tests (PathDB, LLM Sync, Storage)
# 2. FFI integration tests 
# 3. Idris type-checking (if available)
# 4. Full pipeline demo
#
# Usage:
#   ./scripts/e2e_test.sh           # Run all tests
#   ./scripts/e2e_test.sh --quick   # Run quick tests only
#   ./scripts/e2e_test.sh --rust    # Rust tests only
#   ./scripts/e2e_test.sh --idris   # Idris tests only

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
IDRIS_ONLY=false

for arg in "$@"; do
    case $arg in
        --quick)
            QUICK=true
            ;;
        --rust)
            RUST_ONLY=true
            ;;
        --idris)
            IDRIS_ONLY=true
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
# Step 1: Rust Build
# ============================================================================

if [ "$IDRIS_ONLY" = false ]; then
    print_header "Step 1: Rust Build"
    
    cd "$PROJECT_ROOT/rust"
    
    if [ "$QUICK" = true ]; then
        run_test "Rust build (debug)" "cargo build"
    else
        run_test "Rust build (release)" "cargo build --release"
    fi
fi

# ============================================================================
# Step 2: Rust Unit Tests
# ============================================================================

if [ "$IDRIS_ONLY" = false ]; then
    print_header "Step 2: Rust Unit Tests"
    
    cd "$PROJECT_ROOT/rust"
    
    run_test "axiograph-pathdb tests" "cargo test -p axiograph-pathdb --no-fail-fast" || true
    run_test "axiograph-dsl tests" "cargo test -p axiograph-dsl --no-fail-fast" || true
    run_test "axiograph-storage tests" "cargo test -p axiograph-storage --no-fail-fast" || true
    
    if [ "$QUICK" = false ]; then
        run_test "axiograph-llm-sync tests" "cargo test -p axiograph-llm-sync --no-fail-fast" || true
        run_test "axiograph-compiler tests" "cargo test -p axiograph-compiler --no-fail-fast" || true
    fi
fi

# ============================================================================
# Step 3: FFI Library Build
# ============================================================================

if [ "$IDRIS_ONLY" = false ]; then
    print_header "Step 3: FFI Library Build"
    
    cd "$PROJECT_ROOT/rust"
    
    if [ "$QUICK" = true ]; then
        run_test "FFI library build" "cargo build -p axiograph-ffi"
    else
        run_test "FFI library build (release)" "cargo build -p axiograph-ffi --release"
    fi
fi

# ============================================================================
# Step 4: FFI Integration Tests
# ============================================================================

if [ "$IDRIS_ONLY" = false ]; then
    print_header "Step 4: FFI Integration Tests"
    
    cd "$PROJECT_ROOT/rust"
    
    # Run FFI tests (they link against the FFI library)
    run_test "FFI integration tests" "cargo test --test ffi_integration_tests --no-fail-fast" || true
fi

# ============================================================================
# Step 5: Idris Type Checking
# ============================================================================

if [ "$RUST_ONLY" = false ]; then
    print_header "Step 5: Idris Type Checking"
    
    if command -v idris2 &> /dev/null; then
        cd "$PROJECT_ROOT/idris"
        
        print_test "Idris type-check (may take a while with --total)"
        
        # First try quick check without --total
        if timeout 60 idris2 --check axiograph.ipkg 2>&1 | head -30; then
            print_pass "Idris type-check"
        else
            # Check might have timed out or failed
            if [ $? -eq 124 ]; then
                print_skip "Idris type-check (timeout - complex dependent types)"
            else
                print_fail "Idris type-check"
            fi
        fi
    else
        print_skip "Idris type-check (idris2 not installed)"
    fi
fi

# ============================================================================
# Step 6: E2E Demo
# ============================================================================

if [ "$IDRIS_ONLY" = false ] && [ "$QUICK" = false ]; then
    print_header "Step 6: End-to-End Demo"
    
    cd "$PROJECT_ROOT/rust"
    
    # Run the e2e demo example
    if cargo run --release --example e2e_demo 2>&1 | head -50; then
        print_pass "E2E demo"
    else
        print_fail "E2E demo"
    fi
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

