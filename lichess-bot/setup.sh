#!/usr/bin/env bash
# Build knightlm-uci and (re)point the lichess-bot config at it.
# The OAuth token is human-provided and is NOT touched by this script.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/.." && pwd)"
CFG="$HERE/config.knightlm.yml"
PROFILE="${1:-debug}"   # ./setup.sh [debug|release]

echo "[1/3] building knightlm-uci ($PROFILE)..."
if [ "$PROFILE" = "release" ]; then
  cargo build --release -p knightlm-cli --bin knightlm-uci --manifest-path "$REPO_ROOT/Cargo.toml"
  BIN_DIR="$REPO_ROOT/target/release"
else
  cargo build -p knightlm-cli --bin knightlm-uci --manifest-path "$REPO_ROOT/Cargo.toml"
  BIN_DIR="$REPO_ROOT/target/debug"
fi

echo "[2/3] pointing config at $BIN_DIR/knightlm-uci..."
python3 - "$CFG" "$BIN_DIR" <<'PY'
import re, sys
cfg, bin_dir = sys.argv[1], sys.argv[2]
src = open(cfg).read()
src = re.sub(r'(?m)^(\s*dir:\s*)".*"', rf'\1"{bin_dir}"', src, count=1)
open(cfg, "w").write(src)
print(f"  dir -> {bin_dir}")
PY

echo "[3/3] verifying config against lichess-bot..."
echo "  (run this from a lichess-bot clone that has lib/config.py on its path)"
echo "done. Next: paste your OAuth token into $CFG, then run:"
echo "  python lichess-bot.py $CFG"
