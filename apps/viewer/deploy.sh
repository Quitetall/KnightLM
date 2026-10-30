#!/usr/bin/env bash
# Build the KnightLM viewer and (re)deploy it into public/ for the static server.
# cargo-leptos wipes public/ each build, so the index.html + _bg.wasm copies
# must be re-applied after every build. This is the single entry point.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

echo "[1/2] cargo leptos build..."
cargo leptos build >/tmp/viewer_build.log 2>&1 || { echo "BUILD FAILED"; tail -20 /tmp/viewer_build.log; exit 1; }

echo "[2/2] deploying index.html + _bg.wasm into public/..."
cp index.html public/index.html
# wasm-bindgen's JS references knightlm-viewer_bg.wasm; cargo-leptos names it knightlm-viewer.wasm
cp public/pkg/knightlm-viewer.wasm public/pkg/knightlm-viewer_bg.wasm

echo "deployed: $(ls public/) | pkg: $(ls public/pkg/ | tr '\n' ' ')"
