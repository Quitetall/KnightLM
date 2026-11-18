---
format: openwarrant/1.0
profile: spec
part_of: doc:SPEC.md
summary: 140. Technology pins, from docs/sas/KnightLM_SAS.md.
---
# 140. Technology pins

## 140. Technology pins

- Rust 2024 edition, `Cargo.lock` pinned. `cozy-chess` =0.3.4: move generation, FEN including Shredder-FEN, UCI helpers; it parses no SAN or PGN — the notation layer is this program's own.
- `burn` =0.21.0 for Track B; CPU ndarray for checks, GPU backends for training.
- Viewer: RustyBrickChess (`apps/rusty-brick-chess`) is the single viewer; Leptos 0.8.x + axum, compiled to WASM; the application SHALL contain no JavaScript or TypeScript. Its Tauri desktop shell wraps the same viewer.
- Track A: HuggingFace `transformers` with LoRA on cloud CUDA GPUs; local hardware for iteration and CPU-verified runs.
- Stockfish: an external process over UCI; no engine binary is bundled or auto-downloaded.
