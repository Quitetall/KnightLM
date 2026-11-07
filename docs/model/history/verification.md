# Scaffold verification

Local verification on 2026-09-16, before any commit:

- `make check-mojo`: exit 0.
- Rust formatting and Clippy (`-D warnings`): passed.
- Rust tests: 24 passed, including the CPU Burn gradient/checkpoint test.
- Python unit tests: 2 passed.
- CLI acceptance: 8 fixture arms and 61 generated semantic probes passed;
  unavailable pretrained backends were correctly refused.
- Four packet variants matched across Rust, Python and Mojo. This is protocol
  parity, not independent chess-engine or model parity.
- A regression test first reproduced acceptance of Chess960 castling under the
  standard ruleset. Explicit standard-FEN parsing and king-origin validation
  fixed the failure; the full gate passed afterward.

External LAMU review produced no verdict. The connector returned `Transport
closed`; a fresh, separately launched stdio process accepted the review request
but exceeded a 240-second response deadline. That process was terminated. Shared
routing and services were not restarted. No external review approval is claimed.

These checks establish scaffold behavior only. They do not establish pretrained
inference, language-model fine-tuning, chess strength, perception accuracy,
complete dead-position adjudication or the fresh-context research hypothesis.
No bulk data/model download, GPU training, commit or push was performed.
