#!/usr/bin/env bash
set -euo pipefail

VERSION="0.5.4"
DESTINATION="${1:-build/tools/mdbook/mdbook}"
BASE_URL="https://github.com/rust-lang/mdBook/releases/download/v${VERSION}"

case "$(uname -s):$(uname -m)" in
  Darwin:arm64)
    ARCHIVE="mdbook-v${VERSION}-aarch64-apple-darwin.tar.gz"
    SHA256="03e8a6d8b13a2971e0b3280affd03b388373c1485e26f73407c3a76b0b1838df"
    ;;
  Darwin:x86_64)
    ARCHIVE="mdbook-v${VERSION}-x86_64-apple-darwin.tar.gz"
    SHA256="a47d7bf0d5d670cff9ee6cce95537cbeb62dc10704d9e7131ffbd13e2b59a5de"
    ;;
  Linux:x86_64)
    ARCHIVE="mdbook-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz"
    SHA256="3f28de05dafca9d0f2eab99c662116b0e37b89b1d96a08f8f430b9eeae958cd7"
    ;;
  Linux:aarch64)
    ARCHIVE="mdbook-v${VERSION}-aarch64-unknown-linux-musl.tar.gz"
    SHA256="753e5c5c363ee8a56972344dcf91466f005a51db84a7aeffe427ae3ef83d6d44"
    ;;
  *)
    echo "error: mdBook ${VERSION} has no pinned installer for $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac

if [[ -x "${DESTINATION}" ]] && [[ "$("${DESTINATION}" --version)" == "mdbook v${VERSION}" ]]; then
  echo "mdBook v${VERSION} already installed at ${DESTINATION}"
  exit 0
fi

TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/axiograph-mdbook.XXXXXX")"
trap 'rm -rf "${TEMP_DIR}"' EXIT

curl \
  --fail \
  --location \
  --silent \
  --show-error \
  --proto '=https' \
  --tlsv1.2 \
  --connect-timeout 15 \
  --max-time 120 \
  --max-filesize 16777216 \
  "${BASE_URL}/${ARCHIVE}" \
  --output "${TEMP_DIR}/${ARCHIVE}"

ACTUAL_SHA256="$(shasum -a 256 "${TEMP_DIR}/${ARCHIVE}" | awk '{print $1}')"
if [[ "${ACTUAL_SHA256}" != "${SHA256}" ]]; then
  echo "error: mdBook archive digest mismatch" >&2
  echo "expected: ${SHA256}" >&2
  echo "actual:   ${ACTUAL_SHA256}" >&2
  exit 1
fi

tar -xzf "${TEMP_DIR}/${ARCHIVE}" -C "${TEMP_DIR}" mdbook
mkdir -p "$(dirname "${DESTINATION}")"
install -m 0755 "${TEMP_DIR}/mdbook" "${DESTINATION}.tmp"
mv "${DESTINATION}.tmp" "${DESTINATION}"

if [[ "$("${DESTINATION}" --version)" != "mdbook v${VERSION}" ]]; then
  echo "error: installed mdBook did not report v${VERSION}" >&2
  exit 1
fi

echo "Installed mdBook v${VERSION} at ${DESTINATION}"
