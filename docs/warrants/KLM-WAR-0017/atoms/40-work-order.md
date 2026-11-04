---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5da-7086-bc7a-30b39f6fad0c
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-050..PKG-054):

1. Scheduler: Swiss pairings/rounds/byes/tiebreaks, knockout bracket with draw rematch/seed,
   double round robin colour swap; persistent queue; pause/resume/step; concurrency and
   per-provider limits.
2. Tournament persistence/standings/combined PGN; resume after restart; per-game logs.
3. Glicko-2: rating periods, before/after ledger, anchors, unrated rules, materialized
   current table rebuildable from history.
4. Leaderboard + metrics: rating/RD/conservative rank, games, last change, legal-move rate,
   retries, tokens, latency, cost, duration; filters; export.
5. Budgets: per-turn/session/tournament caps; pause at cap; usage display.
6. UI + tests for all formats, resume, ratings, unrated rules, budgets.

Seams:

- Matches remain the single game engine; tournaments schedule them.
- Ratings consume only final match results with the standard-v1 flag and failure reason.
- The leaderboard is a projection of the ledger, not a truth store.

Autonomy and escalation:

Tier T2. Escalate changes to pairing/tiebreak defaults, unrated rules, or the rating pool
policy. The performer owns scheduler internals and UI within the Assistant.

Rollback:

Tournaments/ratings are additive. If the scheduler is reverted, completed matches and the
ledger remain; no chess data is touched.
