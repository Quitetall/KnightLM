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

[ "$SKIP_BUILD" = 1 ] || { echo "[1/4] cargo build --release"; (cd "$APPDIR" && cargo build --release --locked); }
test -x "$BIN" || { echo "missing $BIN (run without --skip-build)" >&2; exit 1; }

# A stale binary installs a working app that is missing schema migrations and
# routes added since it was built, so say so loudly rather than shipping it.
if [ -n "$(find "$APPDIR/src" "$APPDIR/frontend/src" "$APPDIR/../../crates" -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)" ]; then
  echo "WARNING: $BIN is older than the newest source file in this checkout;" >&2
  echo "         it may lack migrations or routes. Rebuild without --skip-build." >&2
fi

# The frontend has to exist before installing; deploy.sh builds it.
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
echo "[2/4] binary -> $BINDIR"
install -d "$BINDIR" "$ASSETS" "$DATADIR"
install -m 0755 "$BIN" "$BINDIR/rusty-brick-chess"

echo "[3/4] frontend -> $ASSETS"
cp -r "$APPDIR/frontend/public/." "$ASSETS/"

if [ "$DESKTOP" = 1 ]; then
  echo "[4/4] desktop entry -> $DATADIR"
  sed -e "s|@BINDIR@|$BINDIR|g" "$HERE/rusty-brick-chess.desktop.in" > "$DATADIR/rusty-brick-chess.desktop"
  chmod 0644 "$DATADIR/rusty-brick-chess.desktop"
else
  echo "[4/4] desktop entry skipped"
fi

cat <<DONE

installed to $PREFIX
  run:      $BINDIR/rusty-brick-chess --open
  data:     \${RBC_DB:-\$XDG_DATA_HOME/rustybrickchess/rusty-brick-chess.db}
  assets:   $ASSETS

Optional at run time:
  STOCKFISH_PATH  a UCI engine, for analysis
  MODEL_URL       an OpenAI-compatible endpoint, for the Assistant tab
DONE
