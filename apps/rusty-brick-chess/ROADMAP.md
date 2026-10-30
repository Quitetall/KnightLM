# RustyBrickChess roadmap

Status against **En Croissant** (the reference desktop app): a database you can
search, a board you can analyse and play from, engines you can configure, and
the same sidebar surfaces, on Linux/macOS/Windows.

**The feature roadmap is complete.** What remains is operator setup (update
signing keys) and technical debt, listed at the end.

## Status

| Surface | State | Notes |
|---|---|---|
| Database: import, search, filters, position search | **Done** | player/event/result/date/Elo/variant filters, exact position search, collections, multi-database, indexes |
| Database at scale | **Done** | 1M-game Lichess slice: stats 38 ms, explorer 45 ms, position search 35 ms |
| Analysis board | **Done** | drag/click, instant/slide, history last-move, board editor, clocks, arrows/circles, variants |
| Engine analysis | **Done** | lines, MultiPV, all engines, report, eval graph, per-engine UCI options, engine dialog + test, UCI console |
| Engine-vs-engine | **Done** | Practice → Engine match with per-engine options, pause/reset, result |
| AI assistant | **Done** | chat dock with OpenAI-style tool calls: analyse, arrows/circles, play moves |
| Analysis polish | **Done** | Lichess cloud eval, Markdown report export, annotate-game pass |
| Tree editing | **Done** | variations, promote/delete, PGN rewrite, annotations |
| Openings | **Done** | local/lichess/masters explorer, ECO names, first-12-ply aggregate index |
| Puzzles | **Done** | Lichess CSV, themes, rating, play-out |
| Repertoires | **Done** | import, training, SM-2 |
| Practice | **Done** | guess-the-move, sparring, engine matches; strength via per-engine Skill Level / UCI_Elo |
| Workspaces | **Done** | Analysis/Database/Openings/Puzzles tabs, rename, reorder, session restore |
| Board customization | **Done** | custom colours, coordinates, 8 built-in sets, custom SVG set upload |
| Files | **Done** | file picker + drag-and-drop import, recent files, Save PGN, desktop Open PGN |
| Accounts | **Done** | Lichess (optional token) and Chess.com links, profile/rating, game import |
| Settings | **Done** | engines, depth, threads/hash, syzygy, theme/pieces/colours, sounds, keybinds, animation, profile, language, vision |
| Desktop shell | **Done** | Tauri window, native menus (Open PGN/database, New window, Reload), installer, CI bundles |
| Packaging & release | **Done** | GitHub Actions matrix builds `.deb`/`.AppImage`/`.dmg`/`.msi` on tags |
| i18n / accessibility | **Done** | English/Spanish UI, board ARIA labels, skip link, live regions, focus rings |
| Auto-update | **Documented** | signing keypair is the release owner's to generate; walkthrough in README |

Rough position: **~95% of the daily En Croissant surface**. The visible gaps are
not features: signed/notarized installers and the updater keypair belong to the
release owner, and a few transient status messages are English-only.

## Verification

Every surface has a headless Chrome suite in the dev harness, all green:

`app2`–`app20`, `puzzle`, `final`, `chat`, `enginetools`, `enginematch`,
`files`, `accounts`, `pieces`, `i18n_a11y`, `boardcustom`, `workspaces2`,
`analysispolish`, `drag`, `layout`, `historysound`, `fps`, `vision`, `anim`,
`workspaces`, `session`, `keybinds`, `practice`, `filters`, `bigdb`,
`desktop_event`.

## Remaining (operator)

1. **Update signing** — generate the keypair, add the two GitHub secrets, add
   `tauri-plugin-updater`, set `plugins.updater` + `createUpdaterArtifacts`, and
   bump `version` per release. Steps: README → Desktop bundles → Auto-update
   signing.
2. **Platform signing** — macOS notarization (Developer ID) and Windows code
   signing certificates, when you have them; CI already produces unsigned
   bundles.

## Technical debt

- `frontend/src/lib.rs` is ~6k lines: split into `views/` modules before the
  next feature surface.
- SQLite is comfortable to ~2–5M games; beyond that, archive mode should move to
  DuckDB/Parquet instead of growing the position index.
- Schema changes rely on `CREATE ... IF NOT EXISTS`; add a migration/version
  table before the next breaking change.
- Session persistence is `localStorage`; move it into the profile if account
  sync becomes server-side.
- Transient status/error messages are not translated.
