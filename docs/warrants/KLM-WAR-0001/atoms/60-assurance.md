---
schema: oh.war/atom/v1
warrant_uuid: 01a1321b-ac88-7a53-b1e1-da29c56db13a
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — the SAS is accepted and pinned
- **scope:** `docs/sas/revisions/0.1.0.toml` and this Warrant's compiled basis.
- **gate:** `gate://software.repo.war-check@1.0.0`
- **evidence:** `war sas status` reports 0.1.0 accepted by a named human; `war check` reports no `sas.unrecorded`.

### OBL-002 — the loop closed with an independent verification
- **scope:** this Warrant's `verifications/` and `resolution.toml`.
- **gate:** `gate://software.repo.war-check@1.0.0`
- **evidence:** a verification whose verifier is not the performer, disposition `established`, and a resolution signed by a human.

## Gate Adequacy

Required at `basic`.

**Adversarial question:** can this Warrant be resolved without a human ever
signing anything? No: `war resolve --response` refuses an agent-kind actor
by kind, and `war sign` needs a terminal or an ssh agent loaded with `-c`.

- **outcome:** no_counterexample

## Residual Risk

- The key custody question (`ssh-add -c`) is the operator's; `war` cannot verify it.
