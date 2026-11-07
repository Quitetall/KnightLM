.PHONY: check check-apps check-mojo gate
check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
	cargo test --workspace --locked -j 2
	cargo test --workspace --all-features --locked -j 2
	cargo build --locked -j 2
	python3 -m unittest discover -s model/parity/python
	python3 -m unittest model.corpus.test_pipeline
	python3 scripts/check_cli.py
	python3 scripts/check_ports.py

# The application crates, which the workspace excludes: clippy, fmt and tests for
# the server and the wasm frontend. `check` alone would stay green while both rot.
check-apps:
	./scripts/check_apps.sh

# Everything a release rests on: the fast tier, the application crates, and every
# browser suite. This is the command that means "the repo is green".
gate: check check-apps
	./scripts/check_all.sh --cdp

check-mojo: check
	python3 scripts/check_ports.py --require-mojo
