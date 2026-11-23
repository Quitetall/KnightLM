# Four-person training plan

Start with the [Docs update](../../work/t-389d/t-389d-docs-contributor-bring-the-rbc-roadmap-up-to.md) and
[Data inventory](../../work/t-a475/t-a475-data-engineer-inventory-sources-for-run-1.md). Contributors can claim those
tickets themselves. This plan names the roles that can pick up the
follow-ups and the evidence they hand to the next person.

| Role | Responsibility | Tracked work |
| --- | --- | --- |
| Project lead | Review the Docs update, settle the first sources and split, and decide when to schedule the full run | [Docs](../../work/t-389d/t-389d-docs-contributor-bring-the-rbc-roadmap-up-to.md), [full Run 1 corpus](../../tickets/t-3458.md) |
| Data Engineer | Inventory sources, build repeatable ingestion and publish the audited snapshot | [Inventory](../../work/t-a475/t-a475-data-engineer-inventory-sources-for-run-1.md), [ETL loop](../../work/t-f724/t-f724-data-engineer-build-a-repeatable-chess-etl-loop.md) |
| Representation/Evaluation Engineer | Own the data format and fixture, then define and score the strength comparison | [Corpus contract](../../work/t-5cd8/t-5cd8-representation-evaluation-engineer-agree-on-the.md), [strength protocol](../../work/t-e135/t-e135-research-contributor-define-a-knightlm-strength.md), [scorecard](../../work/t-c199/t-c199-representation-evaluation-engineer-implement-the.md) |
| MLOps/CV Engineer | Own the training command, fixture tests, bounded GPU run, and checkpoint reload | [Training entry point](../../work/t-ea98/t-ea98-mlops-cv-engineer-make-the-trainer-runnable-from.md), [joined loop smoke test](../../work/t-ab7d/t-ab7d-mlops-cv-engineer-test-the-joined-source-to.md) |

## Handoffs

1. **Agree on the interface.** Once Docs and Data are underway, Representation/
   Evaluation shares the small manifest and fixture with Data and MLOps. The
   strength protocol can be drafted independently. The lead records the agreed
   version after the two consumers review it.
2. **Build the two halves in parallel.** Data ingests a PGN archive and a
   read-only database export into a new, audited snapshot. MLOps makes the
   trainer accept that contract and tests it against the shared fixture.
   Representation/Evaluation builds the scorecard against fixed predictions.
3. **Run the joined loop once at small scale.** MLOps shows that adding a source
   produces a new corpus and that the same manifest reaches a bounded training
   run. Data reviews the provenance and split. Representation/Evaluation checks
   the holdout and scores. The lead reviews the commands, manifests, checkpoint
   reload, and any unresolved blockers before approving full Run 1.

The first end-to-end target is one command that takes a source configuration,
prepares or reuses a verified corpus snapshot, and runs training when explicitly
requested. It should accept new data without duplicating rows, refuse bad data
before training, and preserve earlier runs. Full Run 1 still needs the
[200,000-game training set and 10,000-game holdout](../../tickets/t-3458.md).
