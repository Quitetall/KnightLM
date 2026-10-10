# Experimental ports

Rust owns the complete implemented pipeline. These are bounded parity slices,
not equivalent inference/training backends.

- Python: strict packet boundary, canonical JSON and piece-placement expansion.
- Mojo 1.0.0: native move-counter/draw-status checks; CPython interoperability
  supplies JSON parsing and shared protocol shape validation. This is not an
  independent Mojo JSON parser or chess engine.

Install Mojo locally, without changing system Python:

```sh
uv venv ports/mojo/.venv
uv pip install --python ports/mojo/.venv/bin/python -r ports/mojo/requirements.txt
```

Run `python3 scripts/check_ports.py --require-mojo` from the repository root.
Without `--require-mojo`, absent Mojo is explicitly reported as unavailable.
The check uses temporary artifact directories and compares byte-exact canonical
packets from Rust and both ports. It also injects unknown fields and versions.

Mojo source targets https://mojolang.org/docs/manual/python/ version 1.0.0.
