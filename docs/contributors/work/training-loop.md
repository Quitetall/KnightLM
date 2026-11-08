# Four-person training plan

Start with the [Docs update](../../tickets/t-389d.md) and
[Data inventory](../../tickets/t-a475.md). Contributors can claim those
tickets themselves. This plan names the roles that can pick up the
follow-ups and the evidence they hand to the next person.

| Role | Responsibility | Tracked work |
| --- | --- | --- |
| Project lead | Review the Docs update, settle the first sources and split, and decide when to schedule the full run | [Docs](../../tickets/t-389d.md), [full Run 1 corpus](../../tickets/t-3458.md) |
| Data Engineer | Inventory sources, build repeatable ingestion and publish the audited snapshot | [Inventory](../../tickets/t-a475.md), [ETL loop](../../tickets/t-f724.md) |
| Representation/Evaluation Engineer | Own the data format and fixture, then define and score the strength comparison | [Corpus contract](../../tickets/t-5cd8.md), [strength protocol](../../tickets/t-e135.md), [scorecard](../../tickets/t-c199.md) |
| MLOps/CV Engineer | Own the training command, fixture tests, bounded GPU run, and checkpoint reload | [Training entry point](../../tickets/t-ea98.md), [joined loop smoke test](../../tickets/t-ab7d.md) |

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
