.PHONY: check check-mojo
check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
	cargo test --workspace --locked -j 2
	cargo build --locked -j 2
	python3 -m unittest discover -s ports/python
	python3 scripts/check_cli.py
	python3 scripts/check_ports.py

check-mojo: check
	python3 scripts/check_ports.py --require-mojo
