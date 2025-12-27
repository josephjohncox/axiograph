#!/bin/bash
# Run all E2E tests for Axiograph

set -e

echo "=================================="
echo "  Axiograph E2E Test Suite"
echo "=================================="
echo ""

cd "$(dirname "$0")/.."

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

run_test() {
    local name=$1
    local cmd=$2
    echo -e "${YELLOW}Running: $name${NC}"
    if $cmd; then
        echo -e "${GREEN}✓ $name passed${NC}"
        return 0
    else
        echo -e "${RED}✗ $name failed${NC}"
        return 1
    fi
}

FAILED=0

# 1. Unit tests for each crate
echo ""
echo "=== Unit Tests ==="

run_test "axiograph-dsl" "cargo test -p axiograph-dsl" || FAILED=$((FAILED+1))
run_test "axiograph-pathdb" "cargo test -p axiograph-pathdb" || FAILED=$((FAILED+1))
run_test "axiograph-storage" "cargo test -p axiograph-storage" || FAILED=$((FAILED+1))
run_test "axiograph-llm-sync" "cargo test -p axiograph-llm-sync" || FAILED=$((FAILED+1))

# 2. Integration tests
echo ""
echo "=== Integration Tests ==="

run_test "integration-tests" "cargo test --test integration_tests" || FAILED=$((FAILED+1))

# 3. LLM Sync E2E tests
echo ""
echo "=== LLM Sync E2E Tests ==="

run_test "llm-sync-e2e" "cargo test -p axiograph-llm-sync --test e2e_tests" || FAILED=$((FAILED+1))

# 4. Doc tests
echo ""
echo "=== Doc Tests ==="

run_test "doc-tests" "cargo test --doc" || FAILED=$((FAILED+1))

# Summary
echo ""
echo "=================================="
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
else
    echo -e "${RED}$FAILED test suite(s) failed${NC}"
fi
echo "=================================="

exit $FAILED
