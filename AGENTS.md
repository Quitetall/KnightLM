# Working in this repository

Ordinary coding needs no Warrant and no ticket. Work as in any repository.
For starter orders and the pull-request workflow, see [CONTRIBUTING.md](CONTRIBUTING.md).
OpenWarrant is an optional kit for tracking work: one item, one format, at
one of three levels its fields decide.

| Level | Made with | Done when |
|---|---|---|
| **Ticket** | `war create "..."` | you say so |
| **Warrant** | `--test "<command>"` | every required current check passes |
| **Order** | `--sign` | required checks pass and a person signs |

## Tracking work (optional)

```bash
war start                                    # take the next ready item; its brief holds the records it cites
make check && war done --note "what you did"  # close it at the first passing test run, before polish
war create "What it accomplishes" --item "..."
war status                                   # progress; --next: whose step; --ready: what can start
war check                                    # record integrity
war context <id> --preset agent.min          # only for more than the brief gave: the tight packet
```

The brief's first line names the worktree, branch and commit: work there and
nowhere else. Workers finish their portion without signing. On an Order, `war done` names
the `war sign` command a person runs and leaves the work awaiting signature. Name each of several
agents with `--as <name>`. `war view prime` lists what is open. Over MCP
(`war admin mcp`) the same commands are `war_start`, `war_done`,
`war_create`, `war_status`, `war_check`, `war_context` and `war_note`.
`war_start` automatically supplies its required task context. Use `war_status`
with `target` to read effective level and state, then a separate `next: true`
call for pending acts. Legacy JSON keys named `ticket` can hold any level.

## Formal profile requirements

For a record using a formal profile such as `delivery` or `decision`, follow
its explicit action gates and the sign-off workflow. Legacy directories use
`docs/warrants/KLM-WAR-NNNN/`. The tool holds five facts:

1. **Your own work is checked by someone else**: `war evidence verify` refuses a verdict whose verifier is the performer.
2. **A disposition comes from the verifier** (`war evidence verify --response`), not from an atom edited by hand.
3. **Unknown is reported as UNKNOWN**: a check that could not run is neither a pass nor a failure.
4. **Generated files are rebuilt, not edited**: change the atoms and run `war admin compile`; a file a closed Warrant pins (`war admin pins --resolved-only`) changes through `war sign correct`.
5. **A checker's input is fixed at its source**: when a check and a document disagree, find which is wrong before changing either.

Approving, closing, accepting a spec, correcting a delivered file and
withdrawing a check are a person's `war sign`; you draft the request, and
`war evidence prepare <alias>` runs your half.

Full rules, the atoms and the sign-off loop:
<https://github.com/Quitetall/OpenWarrant/blob/main/docs/agents/legacy-warrant-workflow.md>,
and `war <command> --help`. `war admin doctor` checks the setup, signing included.

<!-- openwarrant:begin -->
<!-- Written by `war admin agents-md --block`, which rewrites these lines. -->
Ordinary coding needs no Warrant and no ticket: work here as in any repository.
Tracked work: run `war start` before reading anything else (the next ready item, or
`war start T8` by id or title words); it prints what the work needs. Don't read
docs/tickets/ directly: it costs far more. Close it at the first passing test run,
before polish: `<tests> && war done --note "what you did"`. `war view prime` lists all.
<!-- openwarrant agents-md: written by war 1.0.0-alpha.8 -->
<!-- openwarrant:end -->
