#!/bin/bash
# Axiograph Setup Script
#
# Sets up local tooling for the Rust runtime + Lean trusted checker release.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                 AXIOGRAPH SETUP                               ║"
echo "╚══════════════════════════════════════════════════════════════╝"

# ============================================================================
# Rust
# ============================================================================

echo ""
echo "━━━ Building Rust workspace ━━━"

cd "$ROOT_DIR"
make rust
make binaries

echo ""
echo "━━━ Optional Rust features ━━━"
echo "  PDF extraction (optional):"
echo "    cd rust && cargo build -p axiograph-ingest-docs --features pdf"
echo "  RDF/OWL parsing (optional):"
echo "    cd rust && cargo build -p axiograph-ingest-rdfowl --features rdf"

# ============================================================================
# Lean
# ============================================================================

echo ""
echo "━━━ Checking Lean tooling ━━━"

if command -v lake &> /dev/null; then
    echo "  ✓ lake found: $(lake --version 2>/dev/null || true)"
    echo "  Building Lean library target..."
    make lean
else
    echo "  ⚠️  lake (Lean) not found. Install via elan:"
    echo "     https://leanprover-community.github.io/get_started.html"
    echo ""
    echo "  Note: building the native Lean executable (make lean-exe) requires a C toolchain."
fi

# ============================================================================
# Verus Setup (Optional)
# ============================================================================

echo ""
echo "━━━ Verus Setup (Optional) ━━━"

if [ -d "$HOME/.verus" ]; then
    echo "  ✓ Verus found at ~/.verus"
else
    echo "  Verus not installed. To install:"
    echo "     git clone https://github.com/verus-lang/verus ~/.verus"
    echo "     cd ~/.verus/source"
    echo "     ./tools/get-z3.sh"
    echo "     source ../tools/activate"
    echo "     vargo build --release"
fi

# ============================================================================
# LLM API Setup
# ============================================================================

echo ""
echo "━━━ LLM API Configuration ━━━"

if [ -n "$OPENAI_API_KEY" ]; then
    echo "  ✓ OPENAI_API_KEY is set"
elif [ -n "$ANTHROPIC_API_KEY" ]; then
    echo "  ✓ ANTHROPIC_API_KEY is set"
elif [ -n "$OLLAMA_HOST" ]; then
    echo "  ✓ OLLAMA_HOST is set"
else
    echo "  ⚠️  No LLM API configured. Set one of:"
    echo "     export OPENAI_API_KEY=sk-..."
    echo "     export ANTHROPIC_API_KEY=sk-..."
    echo "     export OLLAMA_HOST=http://127.0.0.1:11434"
    echo ""
    echo "  You can also set model selection:"
    echo "     export OPENAI_MODEL=gpt-4-turbo-preview"
    echo "     export ANTHROPIC_MODEL=claude-3-opus-20240229"
    echo "     export OLLAMA_MODEL=llama3.2"
fi

# ============================================================================
# Create .env template
# ============================================================================

echo ""
echo "━━━ Creating .env Template ━━━"

cat > .env.example << 'EOF'
# Axiograph Environment Configuration

# LLM Provider (choose one)
# OPENAI_API_KEY=sk-...
# OPENAI_MODEL=gpt-4-turbo-preview
# OPENAI_BASE_URL=https://api.openai.com/v1

# ANTHROPIC_API_KEY=sk-ant-...
# ANTHROPIC_MODEL=claude-3-opus-20240229

# OLLAMA_HOST=http://127.0.0.1:11434
# OLLAMA_MODEL=llama3.2

# Database
# AXIOGRAPH_DATA_DIR=./data
# AXIOGRAPH_LOG_LEVEL=info

# Verus (optional)
# VERUS_ROOT=$HOME/.verus
EOF

echo "  ✓ Created .env.example"

if [ ! -f .env ]; then
    cp .env.example .env
    echo "  ✓ Created .env (edit to add your API keys)"
fi

# ============================================================================
# Summary
# ============================================================================

echo ""
echo "━━━ Setup Complete ━━━"
echo ""
echo "Next steps:"
echo "  1. Edit .env with your API keys (optional)"
echo "  2. Run tests: make rust-test"
echo "  3. Try the REPL: ./bin/axiograph repl"
echo ""
echo "Optional:"
echo "  - Install Verus for formal verification"
echo ""
