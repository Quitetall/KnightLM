#!/usr/bin/env bash
# Install RustyBrickChess for the current user.
#
# The app is one binary plus the static frontend it serves: the binary mounts
# the API and falls back to `frontend/public` for everything else, so the two
# have to travel together. This puts the binary and its assets side by side
# under a prefix, and optionally a desktop entry that runs `--open`.
#
#   packaging/install.sh                      # ~/.local by default
#   packaging/install.sh --prefix /usr/local  # system-wide (may need sudo)
#   packaging/install.sh --no-desktop         # skip the .desktop entry
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPDIR="$(cd "$HERE/.." && pwd)/apps/rusty-brick-chess"
BIN="$APPDIR/target/release/rusty-brick-chess"

PREFIX="${PREFIX:-$HOME/.local}"
DESKTOP=1
SKIP_BUILD=0

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --no-desktop) DESKTOP=0; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ "$SKIP_BUILD" != 1 ]; then
  echo "[1/5] build the server and browser frontend"
  (cd "$APPDIR" && cargo build --release --locked)
  "$APPDIR/frontend/deploy.sh"
fi
test -x "$BIN" || { echo "missing $BIN (run without --skip-build)" >&2; exit 1; }

# A stale binary installs a working app that is missing schema migrations and
# routes added since it was built, so say so loudly rather than shipping it.
if [ -n "$(find "$APPDIR/src" "$APPDIR/frontend/src" "$APPDIR/../../crates" -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)" ]; then
  echo "WARNING: $BIN is older than the newest source file in this checkout;" >&2
  echo "         it may lack migrations or routes. Rebuild without --skip-build." >&2
fi

# Also validate the prebuilt bundle when --skip-build is used.
if [ ! -s "$APPDIR/frontend/public/index.html" ]; then
  echo "missing frontend/public/index.html - run frontend/deploy.sh first" >&2
  exit 1
fi
for f in pkg/rbc-frontend.js pkg/rbc-frontend_bg.wasm; do
  test -s "$APPDIR/frontend/public/$f" || { echo "missing frontend/public/$f" >&2; exit 1; }
done

BINDIR="$PREFIX/bin"
# `static_dir()` resolves `<exe>/../share/rustybrickchess/frontend` first, so
# the assets go there and the binary finds them without any environment.
ASSETS="$PREFIX/share/rustybrickchess/frontend"
DATADIR="$PREFIX/share/applications"
echo "[2/5] binary -> $BINDIR"
install -d "$BINDIR" "$ASSETS" "$DATADIR"
install -m 0755 "$BIN" "$BINDIR/rusty-brick-chess"

echo "[3/5] frontend -> $ASSETS"
cp -r "$APPDIR/frontend/public/." "$ASSETS/"

# The model server's Python code, so "Play AI" can download and run the model. The list is
# packaging/model-runtime.files (checked by scripts/check_model_runtime_files.py); the app finds
# it at <exe>/../share/rustybrickchess/model-runtime.
RUNTIME="$PREFIX/share/rustybrickchess/model-runtime"
echo "[4/5] model server code -> $RUNTIME"
rm -rf "$RUNTIME"
while IFS= read -r rel; do
  case "$rel" in ''|'#'*) continue ;; esac
  install -D -m 0644 "$HERE/../$rel" "$RUNTIME/$rel"
done < "$HERE/model-runtime.files"

if [ "$DESKTOP" = 1 ]; then
  echo "[5/5] desktop entry -> $DATADIR"
  sed -e "s|@BINDIR@|$BINDIR|g" "$HERE/rusty-brick-chess.desktop.in" > "$DATADIR/rusty-brick-chess.desktop"
  chmod 0644 "$DATADIR/rusty-brick-chess.desktop"
else
  echo "[5/5] desktop entry skipped"
fi

cat <<DONE

installed to $PREFIX
  run:      $BINDIR/rusty-brick-chess --open
  data:     \${RBC_DB:-\$XDG_DATA_HOME/rustybrickchess/rusty-brick-chess.db}
  assets:   $ASSETS

Play AI:  open the app, choose "Play AI". The test bot works at once; "KnightLM model"
          offers a one-click download (about 1 GB, checked) and needs Python 3.10 or newer.

Optional at run time:
  STOCKFISH_PATH  a UCI engine, for analysis
  MODEL_URL       an OpenAI-compatible endpoint, for the Assistant tab
DONE
