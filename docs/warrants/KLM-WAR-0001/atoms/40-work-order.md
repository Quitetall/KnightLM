---
schema: oh.war/atom/v1
warrant_uuid: 01a1321b-ac88-7a53-b1e1-da29c56db13a
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. The SAS accepted at 0.1.0 (`war sas propose 0.1.0`, then a human's `war sign 0.1.0 --ssh-sign`).
2. This Warrant authorized (`war authorize KLM-WAR-0001`, then `war sign KLM-WAR-0001 --ssh-sign`).
3. Gate evidence recorded (`war evidence record KLM-WAR-0001`).
4. An independent verification ingested (`war verify KLM-WAR-0001 --performer <you>` handed to a separate context; `war verify KLM-WAR-0001 --response <file>`).
5. This Warrant resolved (`war resolve KLM-WAR-0001`, then `war sign KLM-WAR-0001 --ssh-sign`).

## Frozen Surfaces

`docs/authority/` — human-written; no tool writes it.

## Premade Instructions

- `war next` before every step; it names whose act comes next.
- Never edit a record to make `war check` go green.

## Autonomy and Escalation

Tier T2: the agent drafts and records; every signature is a human's.

## Rollback

Delete `docs/warrants/KLM-WAR-0001/`; nothing else depends on it yet.
