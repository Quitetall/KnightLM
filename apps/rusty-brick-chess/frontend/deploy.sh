#!/usr/bin/env bash
# Build the RustyBrickChess frontend and deploy it into public/.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"
echo "[1/2] cargo leptos build..."
cargo leptos build --frontend-only --cargo-locked
echo "[2/2] deploying..."
# cargo-leptos owns public/ and may remove the entrypoint, so copy it last
# and verify every file the browser needs before declaring success.
cp index.html public/index.html
rm -rf public/sounds
cp -r assets/sounds public/sounds
test -s public/index.html
for f in pkg/rbc-frontend.js pkg/rbc-frontend.wasm; do
  if [ ! -s "public/$f" ]; then
    echo "MISSING public/$f"; exit 1
  fi
done
# cargo-leptos emits rbc-frontend.wasm; the JS loader asks for the _bg name.
# Copy it EVERY time: guarding on "does _bg exist" left the browser running
# whatever was deployed first, which is how a freshly built app kept showing an
# old navigation bar.
cp -f public/pkg/rbc-frontend.wasm public/pkg/rbc-frontend_bg.wasm
test -s public/pkg/rbc-frontend_bg.wasm
cmp -s public/pkg/rbc-frontend.wasm public/pkg/rbc-frontend_bg.wasm || {
  echo "the _bg copy does not match the built wasm"; exit 1; }
echo "deployed: $(ls public/ | tr '\n' ' ')| pkg: $(ls public/pkg/ | tr '\n' ' ')"
