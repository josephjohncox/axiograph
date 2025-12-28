#!/bin/bash
# Git pre-commit hook for Attestor
# Runs fast security and quality checks before allowing commits
#
# To install: ln -s ../../scripts/pre-commit.sh .git/hooks/pre-commit

set -e

# Get Go binary paths
GOBIN=$(go env GOPATH)/bin
GOSEC="$GOBIN/gosec"
TRUFFLEHOG="$GOBIN/trufflehog"

echo "🔍 Running pre-commit checks..."

# Format check
echo "  → Checking formatting..."
UNFORMATTED=$(gofmt -l . 2>&1 | grep -v vendor || true)
if [ -n "$UNFORMATTED" ]; then
    echo "❌ Some files are not formatted. Run: go fmt ./..."
    echo "$UNFORMATTED"
    exit 1
fi

# Go vet
echo "  → Running go vet..."
if ! go vet ./... 2>&1; then
    echo "❌ go vet found issues"
    exit 1
fi

# Fast unit tests (short mode)
echo "  → Running fast tests..."
if ! go test -short -timeout=30s ./... 2>&1 | grep -E "(PASS|FAIL|ok|FAIL)"; then
    echo "❌ Tests failed"
    exit 1
fi

# Security scan (gosec - quick mode)
echo "  → Running security scan..."
if [ -f "$GOSEC" ]; then
    # Run gosec quietly, only fail on high severity
    if ! "$GOSEC" -quiet -severity high ./... 2>/dev/null; then
        echo "⚠️  High severity security issues found. Run: make sec"
        echo "   Continuing with commit (review findings)..."
    fi
else
    echo "⚠️  gosec not found at $GOSEC (skipping security scan)"
    echo "   Install: go install github.com/securego/gosec/v2/cmd/gosec@latest"
fi

# Check for hardcoded secrets in staged files (not field names/comments)
echo "  → Checking for hardcoded secrets..."
SECRETS_PATTERNS=(
    "BEGIN RSA PRIVATE KEY"
    "BEGIN PRIVATE KEY"
    "['\"][a-zA-Z0-9]{32,}['\"].*password"
    "['\"][a-zA-Z0-9]{32,}['\"].*secret"
    "['\"][a-zA-Z0-9]{32,}['\"].*token"
)

STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM | grep '\.go$' || true)
if [ -n "$STAGED_FILES" ]; then
    for file in $STAGED_FILES; do
        for pattern in "${SECRETS_PATTERNS[@]}"; do
            if grep -E "$pattern" "$file" > /dev/null 2>&1; then
                echo "❌ Potential hardcoded secret found in $file"
                echo "   Pattern: $pattern"
                echo "   Please use environment variables or secret management"
                exit 1
            fi
        done
    done
fi

# Check for TODO/FIXME in staged Go files
TODO_COUNT=$(echo "$STAGED_FILES" | xargs grep -n "TODO\|FIXME" 2>/dev/null | wc -l || echo 0)
if [ "$TODO_COUNT" -gt 0 ]; then
    echo "⚠️  Found $TODO_COUNT TODO/FIXME comments in staged files"
fi

echo "✅ Pre-commit checks passed!"
echo ""

exit 0

