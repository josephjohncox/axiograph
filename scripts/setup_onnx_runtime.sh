#!/bin/bash
set -euo pipefail

# Install ONNX build/runtime deps into a local venv.
#
# Usage:
#   ALLOW_ONNX_PIP_INSTALL=1 ./scripts/setup_onnx_runtime.sh
#   source .venv-onnx/bin/activate

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VENV_DIR="$ROOT_DIR/.venv-onnx"
PYTHON="${PYTHON:-python3}"

if [ "${ALLOW_ONNX_PIP_INSTALL:-0}" != "1" ]; then
  echo "error: setup_onnx_runtime.sh installs packages from PyPI; rerun with ALLOW_ONNX_PIP_INSTALL=1" >&2
  exit 2
fi

if ! command -v "$PYTHON" >/dev/null 2>&1; then
  echo "error: Python interpreter not found: $PYTHON" >&2
  exit 2
fi

"$PYTHON" -m venv "$VENV_DIR"
source "$VENV_DIR/bin/activate"

python -m pip install --upgrade pip
python -m pip install onnx onnxruntime numpy

echo "ok: onnx env ready at $VENV_DIR"
