# Releasing a desktop pre-release

Releases are cut by a human. Nothing publishes itself. This describes what
`.github/workflows/release.yml` does today.

## Steps

1. **Merge to `main` first.** The workflow builds whatever the tag points at. Check that the
   version in `apps/rusty-brick-chess/src-tauri/tauri.conf.json` is what you want (installer file
   names carry it: the `v0.1.0-prealpha.4` assets are named `0.1.0`).
2. **A human pushes a tag** matching `v*`, for example `v0.1.0-prealpha.5`. A tag containing `-`
   is marked a pre-release. (The workflow can also be started by hand with `workflow_dispatch`.)
3. **The workflow builds drafts.** It runs one `bundle` job per platform: `ubuntu-22.04`,
   `macos-14` and `windows-2022`. Each builds the frontend, downloads and checks the pinned
   Stockfish 19 (`scripts/fetch_bundled_stockfish.sh`, smoke-tested with `uci`), then
   `tauri-action` builds the installers and attaches them to a **draft** release for the tag,
   with the body set in the workflow file.
4. **Verify the assets.** Linux: `.AppImage`, `.deb`, `.rpm`. Windows: `-setup.exe`, `.msi`.
   Compare against the previous release (`gh release view <tag> --json assets`).
5. **Edit the notes.** The draft body is generic. State what changed, and keep the honest
   limits true for this build (see below).
6. **Publish the draft** (GitHub UI, or `gh release edit <tag> --draft=false`).

## Known gaps

- **No macOS build.** GitHub assigned no macOS runner, so the `macos-14` job is cancelled and
  the workflow run shows as cancelled even though the Linux and Windows jobs succeed. No macOS
  installer is published; Mac users build from source ([Running RBC](running.md)).
- **Unsigned.** There is no code signing and no updater. Windows SmartScreen warns; the user
  chooses "More info > Run anyway".
- **Hand-testing.** Only Linux has been run by a person. The Windows installers come from CI.
  Do not claim otherwise in the notes until someone has installed them.
- Playing the real model is opt-in and not part of the installer; the notes should say it plays
  at about random strength and does not chat.
