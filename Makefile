.PHONY: check check-apps check-desktop check-mojo check-secrets gate rbc-desktop rbc-desktop-bundle rbc-desktop-smoke
check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
	cargo test --workspace --locked -j 2
	cargo test --workspace --all-features --locked -j 2
	cargo build --locked -j 2
	python3 -m unittest discover -s model/parity/python
	python3 -m unittest model.corpus.test_pipeline model.corpus.test_v3 model.corpus.test_reasoning model.corpus.test_s0
	python3 -m unittest model.tests.test_materialize model.tests.test_train_run model.tests.test_pipeline model.tests.test_prompt
	python3 -m unittest model.eval.test_eval model.eval.test_oracles model.eval.test_explain model.eval.test_playmode model.eval.test_s0
	python3 -m unittest model.serve.test_play model.serve.test_chat model.serve.test_coach
	python3 -m unittest model.tokenizer.test_tokenizer model.tokenizer.test_hf
	python3 -m unittest model.perceiver.test_perceiver
	python3 scripts/check_cli.py
	python3 scripts/check_ports.py
	python3 scripts/test_check_pins.py
	python3 scripts/check_pins.py
	python3 scripts/test_check_layers.py
	python3 scripts/check_layers.py

# The application crates, which the workspace excludes: clippy, fmt and tests for
# the server and the wasm frontend. `check` alone would stay green while both rot.
# Scans the whole history of this checkout for committed credentials; the same
# policy runs in CI as the `secret-scan` job. Needs the gitleaks binary.
check-secrets:
	gitleaks detect --source . --config .gitleaks.toml --redact --no-banner --log-opts="--all"

check-apps:
	./scripts/check_apps.sh

# Everything a release rests on: the fast tier, the application crates, and every
# browser suite. This is the command that means "the repo is green".
gate: check check-apps
	./scripts/check_all.sh --cdp

check-mojo: check
	python3 scripts/check_ports.py --require-mojo

# The RustyBrickChess desktop app (Tauri). Needs the frontend built first
# (`apps/rusty-brick-chess/frontend/deploy.sh`) and the Tauri system libraries
# (docs/app/running.md).
rbc-desktop:
	cd apps/rusty-brick-chess/src-tauri && cargo run --release

# Installers for this platform: .deb and .AppImage on Linux (needs `cargo install tauri-cli --version "^2"`).
rbc-desktop-bundle:
	cd apps/rusty-brick-chess/src-tauri && cargo tauri build

# fmt, clippy and the virtual-display smoke test for the desktop shell (Linux).
check-desktop:
	cd apps/rusty-brick-chess/src-tauri && cargo fmt --all -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo build --locked
	./scripts/rbc_desktop_smoke.sh

rbc-desktop-smoke: check-desktop
