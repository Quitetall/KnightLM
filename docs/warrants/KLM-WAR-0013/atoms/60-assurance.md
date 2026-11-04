---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a580-74f0-be4c-a5ef4787ae41
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance



## Gate Adequacy

Required at `basic`. **Adversarial question:** how could this look done and not be?
Each obligation pairs a positive claim with a refusal: a stale revision, a locked board, a
missing hash, an unapproved write, or a dropped required section. The package test in
`openwarrant/packages.json` is the executable gate; the obligation text is the contract.


## Obligations

### OBL-001: the same input compiles to the same bytes

- **scope:** one context spec, one input, twenty compilations in one process. It
  does not claim stability across a tokenizer change or a spec revision.
- **evidence:** `same_input_twenty_times_is_byte_identical` and
  `input_json_key_order_does_not_matter` in `crates/context/tests/compile.rs`.

### OBL-002: the hash covers the evidence, not just the words

- **scope:** identical text with a different evidence fingerprint must hash
  differently. Whether that fingerprint is *correct* is out of scope.
- **evidence:** `evidence_fingerprint_changes_hash` (`crates/context/tests/compile.rs`).

### OBL-003: an over-budget compile names the sections it could not fit

- **scope:** the failure path when required sections exceed the budget.
- **evidence:** `required_section_budget_failure_names_sections`
  (`crates/context/tests/compile.rs`).

### OBL-004: the CLI compiles, validates and hashes without a server

- **scope:** `rbc-context` subcommands operating on files. It says nothing about the
  HTTP compile route, which is a different caller of the same library.
- **evidence:** the `knightlm-context` binary's own tests plus `scripts/check_cli.py`
  in the repository's `make check`.

## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
