#!/usr/bin/env bash
# Fetch the official Stockfish release build for one OS, verify its pinned sha256, and place the
# binary where Tauri bundles it (apps/rusty-brick-chess/src-tauri/bundled-engine/ -> engine/).
#
#   scripts/fetch_bundled_stockfish.sh linux|macos|windows [DEST_DIR]
#
# Which build: Stockfish 19's "universal" assets. Each is ONE binary that picks the best
# instruction set (AVX-512 / AVX2 / SSE4.1 ...) when it starts, so it runs on every x86-64 CPU
# (and Apple silicon for macOS), not only on machines with AVX2. The cost is size (~100 MB, the
# NNUE networks are inside). A plain "x86-64-avx2" build would be smaller but crashes on CPUs
# without AVX2. Hashes below were computed from the downloaded files, not copied from a page.
set -euo pipefail

TAG=sf_19
BASE="https://github.com/official-stockfish/Stockfish/releases/download/$TAG"
here="$(cd "$(dirname "$0")" && pwd)"
os="${1:?usage: $0 linux|macos|windows [DEST_DIR]}"
dest="${2:-$here/../apps/rusty-brick-chess/src-tauri/bundled-engine}"

case "$os" in
  linux)
    asset=stockfish-linux-x86-64-universal.tar.gz
    sha=9defc0d4e55d49c65a6d042f3e571a39fcea499ade6dbe741b53b8c65e03611f
    member=stockfish/stockfish-linux-x86-64-universal
    out=stockfish ;;
  macos)
    asset=stockfish-macos-universal.tar.gz
    sha=a1f0e3bcc5a6927a11fe6fc8e54a779754645f3c2bae2cf13420fd1957adaa77
    member=stockfish/stockfish-macos-universal
    out=stockfish ;;
  windows)
    asset=stockfish-windows-x86-64-universal.zip
    sha=3c8bf1f9ea66a09350a40df4f632288285ac206d99f33ab5842c408fc30b48a7
    member=stockfish/stockfish-windows-x86-64-universal.exe
    out=stockfish.exe ;;
  *) echo "unknown OS $os" >&2; exit 2 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
timeout 600 curl --fail --silent --show-error --location --retry 3 -o "$work/$asset" "$BASE/$asset"
if command -v sha256sum >/dev/null; then got="$(sha256sum "$work/$asset" | cut -d' ' -f1)"
else got="$(shasum -a 256 "$work/$asset" | cut -d' ' -f1)"; fi
if [ "$got" != "$sha" ]; then
  echo "sha256 mismatch for $asset: got $got, pinned $sha" >&2
  exit 1
fi
mkdir -p "$dest"
case "$asset" in
  *.zip) unzip -q -o -j "$work/$asset" "$member" -d "$work" ;;
  *) tar -xzf "$work/$asset" -C "$work" "$member" ;;
esac
cp "$work/$member" "$dest/$out" 2>/dev/null || cp "$work/$(basename "$member")" "$dest/$out"
chmod +x "$dest/$out"
echo "bundled $asset ($sha) -> $dest/$out"
