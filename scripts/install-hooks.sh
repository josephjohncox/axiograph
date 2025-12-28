#!/bin/bash
# Install Git hooks for Attestor

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$PROJECT_ROOT/.git/hooks"

echo "Installing Git hooks..."

# Create hooks directory if it doesn't exist
mkdir -p "$HOOKS_DIR"

# Install pre-commit hook
if [ -f "$HOOKS_DIR/pre-commit" ] && [ ! -L "$HOOKS_DIR/pre-commit" ]; then
    echo "⚠️  Existing pre-commit hook found. Backing up to pre-commit.bak"
    mv "$HOOKS_DIR/pre-commit" "$HOOKS_DIR/pre-commit.bak"
fi

ln -sf "$SCRIPT_DIR/pre-commit.sh" "$HOOKS_DIR/pre-commit"
echo "✓ Installed pre-commit hook"

# Make scripts executable
chmod +x "$SCRIPT_DIR/pre-commit.sh"

echo ""
echo "✅ Git hooks installed successfully!"
echo ""
echo "The pre-commit hook will run:"
echo "  - Code formatting checks"
echo "  - go vet"
echo "  - Fast tests"
echo "  - Security scan (gosec)"
echo "  - Secret detection"
echo ""
echo "To skip hooks (not recommended): git commit --no-verify"
echo ""

