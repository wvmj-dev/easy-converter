#!/usr/bin/env bash
# Downloads the PDFium build bundled with the Linux packages into
# src-tauri/resources/pdfium/ and checks its SHA-256.
#
# The build is required at compile time (it is a bundle resource), so run this once
# before `npm run tauri dev`, `cargo test` or `npm run tauri build`.
# PDFium is not in the Debian or Fedora repositories; the binaries come from
# https://github.com/bblanchon/pdfium-binaries (BSD-3-Clause).
set -euo pipefail

VERSION="8165"
SHA256="ba7865e16c0271daab75256790be06c26272190afe6e2a70eae0bb39bb99089c"
URL="https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${VERSION}/pdfium-linux-x64.tgz"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="$root/src-tauri/resources/pdfium"

if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "Only x86_64 Linux is supported." >&2
  exit 1
fi

if [[ -f "$dest/libpdfium.so" ]] && grep -qx "BUILD=$VERSION" "$dest/VERSION" 2>/dev/null; then
  echo "PDFium chromium/$VERSION is already in $dest"
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "Downloading PDFium chromium/$VERSION..."
curl --proto '=https' --tlsv1.2 -fsSL -o "$work/pdfium.tgz" "$URL"
echo "$SHA256  $work/pdfium.tgz" | sha256sum --check --quiet

mkdir -p "$work/extract"
tar -xzf "$work/pdfium.tgz" -C "$work/extract" lib/libpdfium.so VERSION LICENSE

rm -rf "$dest"
mkdir -p "$dest"
install -m 0644 "$work/extract/lib/libpdfium.so" "$dest/libpdfium.so"
install -m 0644 "$work/extract/VERSION" "$dest/VERSION"
install -m 0644 "$work/extract/LICENSE" "$dest/LICENSE"
echo "PDFium chromium/$VERSION installed in $dest"
