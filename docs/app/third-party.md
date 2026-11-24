# Third-party software shipped with the desktop app

## Stockfish 19 (chess engine)

- **What:** the unmodified official release build `stockfish-<os>-universal` of Stockfish 19,
  bundled as `engine/stockfish` (`stockfish.exe` on Windows) in the `.AppImage`, `.deb`, `.rpm`,
  `.exe`/`.msi` and `.dmg`.
- **Licence:** GNU General Public License version 3. The full text ships with the app in
  `engine/COPYING-STOCKFISH-GPLv3.txt` and in this repository at
  `apps/rusty-brick-chess/src-tauri/bundled-engine/COPYING-STOCKFISH-GPLv3.txt`.
- **Source:** <https://github.com/official-stockfish/Stockfish/tree/sf_19> (tag `sf_19`); the
  binaries are from <https://github.com/official-stockfish/Stockfish/releases/tag/sf_19>.
  Authors: <https://github.com/official-stockfish/Stockfish/blob/sf_19/AUTHORS>.
- **How it is used:** RustyBrickChess (AGPL-3.0-only) starts Stockfish as a separate process and
  speaks UCI over its standard input and output; it is not linked in. You can swap it for any
  UCI engine with `STOCKFISH_PATH` or in Settings.
- **Integrity:** the release workflow checks a pinned sha256 for each asset before bundling
  (`scripts/fetch_bundled_stockfish.sh`).
- The notice that ships with the app is
  `apps/rusty-brick-chess/src-tauri/bundled-engine/THIRD_PARTY_NOTICE.txt`.

## CPython (downloaded on demand, not bundled)

If you press **Get the model** and no Python 3.10 to 3.13 is installed, the app downloads a
standalone CPython from [python-build-standalone](https://github.com/astral-sh/python-build-standalone)
(Python Software Foundation License; the build bundles components under their own permissive
licences, listed in the project's release notes). The packages it then installs with pip (PyTorch,
Transformers, PEFT and others) carry their own licences; see `model/serve/requirements.txt`.
