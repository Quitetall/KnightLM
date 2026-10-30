#!/usr/bin/env bash
# Build the RustyBrickChess frontend and deploy it into public/.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"
echo "[1/2] cargo leptos build..."
cargo leptos build >/tmp/rbc_frontend_build.log 2>&1 || { echo "BUILD FAILED"; tail -25 /tmp/rbc_frontend_build.log; exit 1; }
echo "[2/2] deploying..."
# cargo-leptos owns public/ and may remove the entrypoint, so copy it last
# and verify every file the browser needs before declaring success.
cp index.html public/index.html
rm -rf public/sounds
cp -r assets/sounds public/sounds
test -s public/index.html
for f in pkg/rbc-frontend.js pkg/rbc-frontend.wasm pkg/rbc-frontend_bg.wasm; do
  if [ ! -s "public/$f" ]; then
    # cargo-leptos emits rbc-frontend.wasm; the JS loader asks for the _bg name.
    if [ "$f" = "pkg/rbc-frontend_bg.wasm" ] && [ -s public/pkg/rbc-frontend.wasm ]; then
      cp public/pkg/rbc-frontend.wasm public/pkg/rbc-frontend_bg.wasm
    else
      echo "MISSING public/$f"; exit 1
    fi
  fi
done
echo "deployed: $(ls public/ | tr '\n' ' ')| pkg: $(ls public/pkg/ | tr '\n' ' ')"
