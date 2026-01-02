#!/bin/bash
set -euo pipefail

# Install ONNX build/runtime deps into a local venv.
#
# Usage:
#   ./scripts/setup_onnx_runtime.sh
#   source .venv-onnx/bin/activate

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VENV_DIR="$ROOT_DIR/.venv-onnx"

python -m venv "$VENV_DIR"
source "$VENV_DIR/bin/activate"

python -m pip install --upgrade pip
python -m pip install onnx onnxruntime numpy

echo "ok: onnx env ready at $VENV_DIR"
