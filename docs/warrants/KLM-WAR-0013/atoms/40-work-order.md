---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a580-74f0-be4c-a5ef4787ae41
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-010..PKG-015):

1. `rbc-context` crate: section model, canonical JSON, rendered prompt, SHA-256, spec/template
   versions, token counts.
2. Position sections: FEN, side, castling/en passant, ASCII, legal moves (sorted SAN+UCI),
   history (SAN, clocks when present), annotations/shapes.
3. Optional evidence: engine (binary hash, depth, threads=1, hash), openings, database,
   tablebase, notes, sibling directory, compare directory.
4. Token budget, deterministic truncation (oldest complete turn first), tokenizer abstraction
   with a versioned default estimate; required sections fail closed.
5. CLI: `compile`, `batch`, `validate`, `hash`, `sections`; JSON and JSONL output; optional
   target/eval fields in batch without inventing values.
6. Golden/property tests: exact bytes/hash, cross-platform stability, map-order stability,
   evidence fingerprinting, budget behavior, CLI round-trip.

Seams:

- `rbc-ai` supplies evidence and owns persistence of the compiled context/hash.
- The CLI and library expose the same `CompiledContext` schema.
- No ambient time, locale, randomness or map iteration order in rendered content.

Autonomy and escalation:

Tier T2. Escalate any change to the section ordering, hash input, or default sections. The
performer owns the internal renderer structure and tokenizer implementation.

Rollback:

The crate is additive. If a spec version is wrong, bump `spec_version`; never rewrite an old
hash. Keep old fixtures as regression tests.
