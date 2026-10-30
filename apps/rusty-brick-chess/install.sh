#!/usr/bin/env bash
# RustyBrickChess installer.
#
# One step: builds the release binary, installs `rustybrickchess` on PATH,
# adds a desktop icon and a menu entry that opens the app in your browser.
#
#   ./install.sh                 interactive wizard
#   ./install.sh --yes           accept the defaults
#   ./install.sh --prefix DIR    install under DIR (default ~/.local)
#   ./install.sh --uninstall     remove the binary, icon and menu entry
#
# Options:
#   --port PORT        default port written into the launcher (8090)
#   --no-desktop       skip the icon and menu entry
#   --purge            with --uninstall, also delete the database/data folder
#   --yes              never prompt

set -euo pipefail

APP_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PREFIX="${PREFIX:-$HOME/.local}"
PORT="${RBC_PORT:-8090}"
ASSUME_YES=0
DESKTOP=1
UNINSTALL=0
PURGE=0

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --no-desktop) DESKTOP=0; shift ;;
    --yes|-y) ASSUME_YES=1; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    --purge) PURGE=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

XDG_DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="$PREFIX/bin"
BIN="$BIN_DIR/rustybrickchess"
ICON_DIR="$XDG_DATA/icons/hicolor/512x512/apps"
ICON="$ICON_DIR/rustybrickchess.png"
DESKTOP_DIR="$XDG_DATA/applications"
DESKTOP_FILE="$DESKTOP_DIR/rustybrickchess.desktop"
DATA_DIR="$XDG_DATA/rustybrickchess"
SHARE_DIR="$PREFIX/share/rustybrickchess"

say() { printf '%s\n' "$*"; }
ask() {
  local prompt="$1" default="$2" answer
  if [ "$ASSUME_YES" -eq 1 ] || [ ! -t 0 ]; then
    printf '%s' "$default"
    return
  fi
  printf '%s [%s]: ' "$prompt" "$default" >&2
  read -r answer
  printf '%s' "${answer:-$default}"
}

if [ "$UNINSTALL" -eq 1 ]; then
  rm -f "$BIN" "$DESKTOP_FILE" "$ICON"
  rm -rf "$SHARE_DIR"
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
  fi
  if [ "$PURGE" -eq 1 ]; then
    rm -rf "$DATA_DIR"
    say "removed RustyBrickChess and its data ($DATA_DIR)"
  else
    say "removed RustyBrickChess (your games in $DATA_DIR were kept)"
  fi
  exit 0
fi

say "RustyBrickChess installer"
say ""

# Interactive defaults (skipped with --yes or a non-interactive shell).
PREFIX=$(ask "Install prefix" "$PREFIX")
PORT=$(ask "Port for the local app" "$PORT")
if [ "$ASSUME_YES" -eq 0 ] && [ -t 0 ]; then
  DESKTOP_DEFAULT=$([ "$DESKTOP" -eq 1 ] && echo "yes" || echo "no")
  DESKTOP_ANSWER=$(ask "Add a desktop icon and menu entry" "$DESKTOP_DEFAULT")
  case "$DESKTOP_ANSWER" in
    y|Y|yes|YES) DESKTOP=1 ;;
    *) DESKTOP=0 ;;
  esac
fi

BIN_DIR="$PREFIX/bin"
BIN="$BIN_DIR/rustybrickchess"

say ""
say "Building the release binary (a few minutes the first time)…"
(cd "$APP_DIR" && cargo build --release)
BINARY="$APP_DIR/target/release/rusty-brick-chess"
if [ ! -x "$BINARY" ]; then
  echo "build did not produce $BINARY" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"
install -m 755 "$BINARY" "$BIN"
say "installed $BIN"

# Bundle the web assets next to the binary so the install is self-contained.
mkdir -p "$SHARE_DIR/frontend"
cp -R "$APP_DIR/frontend/public/." "$SHARE_DIR/frontend/"
say "installed web assets $SHARE_DIR/frontend"

if [ "$DESKTOP" -eq 1 ]; then
  mkdir -p "$ICON_DIR" "$DESKTOP_DIR"
  install -m 644 "$APP_DIR/src-tauri/icons/icon.png" "$ICON"
  cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=RustyBrickChess
GenericName=Chess Database
Comment=Local chess database, analysis board, openings, puzzles and training
Exec=env RBC_ADDR=127.0.0.1:$PORT "$BIN" --open
Icon=rustybrickchess
Terminal=false
Categories=Game;BoardGame;
Keywords=chess;pgn;analysis;openings;puzzles;
StartupNotify=false
EOF
  chmod 644 "$DESKTOP_FILE"
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
  fi
  say "installed menu entry $DESKTOP_FILE"
fi

say ""
say "Done."
say ""
say "  Run:        rustybrickchess --open"
say "  Or click:   RustyBrickChess in your application menu"
say "  Data:       $DATA_DIR"
if ! printf '%s' ":$PATH:" | grep -q ":$BIN_DIR:"; then
  say ""
  say "Note: $BIN_DIR is not on your PATH. Add this to your shell profile:"
  say "  export PATH=\"$BIN_DIR:\$PATH\""
fi
if ! command -v stockfish >/dev/null 2>&1; then
  say ""
  say "Stockfish was not found on PATH. Analysis needs a UCI engine;"
  say "install Stockfish or set STOCKFISH_PATH, or pick it in Settings."
fi
