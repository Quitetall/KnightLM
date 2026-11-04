---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8cb-70fa-8cfe-03c41eb0d4dd
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — the viewer renders and navigates every Phase-2 fixture
- **scope:** the whole shared fixture battery through ingest tests (HTTP)
- **evidence:** nightly-style test walk: per-format check list rendering the same canonical
state as the parser; annotated fixtures show their comments
### OBL-002 — unparseable input produces an explicit, positioned error
- **scope:** planted malformed pastes per format
- **evidence:** each renders a positioned error, never a hollow board or wrong position
### OBL-003 — every emitted format is exportable via the menu
- **scope:** the export list vs the notation crate's exporter inventory from W-0004
- **evidence:** walk test: per-format export round-trips through http accept, re-parse
identical canonical state
### OBL-004 — zero JavaScript/TypeScript in the shipped viewer
- **scope:** all of code/apps/viewer/ and its build pipeline
- **evidence:** a repo check: no .js/.ts/.mjs/.jsx/.tsx source files and no package-manifest/
node_modules in the app bundle; Leptos/axum tooling only
### OBL-005 — a teammate loads, probes, and exports unaided
- **scope:** one run by an evaluation-lane member (a second by the sponsor if any)
- **evidence:** recorded usability note in this Warrant: task completion, confusion points,
suggestion list; anything breaking an obligation is fixed before resolve

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could the 'Rust-only' rule be satisfied by text scans while a template embeds inline JS?

- **outcome:** no — OBL-004 scans the BUILT bundle definition and the source tree for script
files and registry artifacts, not just file types; the plant test runs inline-script finder on fixture templates.

**Adversarial question:** Could exports pass the menu while failing hidden edge cases?

- **outcome:** no — OBL-003 re-parses via accept; identical canonical state required.


## Residual Risk

Leptos 0.8 + cargo-leptos are a young toolchain; framework churn risk is
bounded by pinning (versions recorded in the Warrant's evidence, updated via
one sponsor-visible decision). Fixture drift will surface as render mismatch
failures in the nightly walk — that is the intended signal; the walk's health
matters more than any single board pixel.
