---
format: openwarrant/1.0
profile: spec
part_of: doc:SPEC.md
summary: 130. Disciplines, from docs/sas/KnightLM_SAS.md.
---
# 130. Disciplines

## 130. Disciplines

Carried from v1, unchanged, and binding on every Warrant.

- The deterministic compiler SHALL be authoritative for rule facts, legality and history; model output and pixels SHALL NOT originate them.
- Every `GameRecord` SHALL carry a `source` and a `history_complete` assertion made by the trusted source; unknown history SHALL NOT be invented; a four-field FEN source SHALL NOT yield `history_complete: true`.
- A failure SHALL be reported, never silently repaired: unparseable notation, observation/ledger mismatch, illegal model move.
- An engine MAY supply teacher labels or post-decision scoring, or play as an opponent; the model's inference path SHALL NOT query an engine.
- Ability SHALL be reported per scoring axis with bounded evidence; a composite winner score SHALL NOT be produced.
- Free prose SHALL NOT be scored; only mechanically checkable structured claims.
