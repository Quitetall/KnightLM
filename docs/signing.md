# Operator checklist: closing a delivery Warrant

Delivery work orders (`KLM-WAR-*`) do not close by themselves. An agent can do
every piece of the work and every check, but **a person signs**, at a terminal,
with no `--yes`. `war sign --help` says it plainly: it "refuses without a TTY
(§27.2: an agent's shell has none)".

So there are two halves. This document is the second one.

## What an agent can finish

```bash
war next                                  # what is ready, and whose step it is
war admin deliver <alias>                 # records what was delivered, at the bytes on disk
war evidence record <alias>               # runs the cited checks, mints receipts in gate-runs/
war evidence verify <alias> --performer <name>   # asks for an independent verifier
war sign resolve --dry-run <alias>        # what is still missing before close-out
```

`war evidence prepare <alias>` (or `--all`) runs the agent's half in order and
lists what is left for a person. It signs nothing.

Two rules that decide whether the output counts:

- **The performer is not the verifier.** `war evidence verify` refuses a verdict
  whose verifier is the performer and writes nothing.
- **UNKNOWN is not a pass.** A check that could not run is reported as UNKNOWN.
  "Four of thirteen established, nine unasked" is an honest close-out request;
  nine claimed is not.

## What only a person can do

At a terminal, in this checkout:

```bash
war sign inbox                    # everything waiting on a human
war sign authorize <alias>        # approve the plan
war sign resolve <alias>          # close the Warrant
war sign sas accept 0.2.1         # accept a spec revision, if one is proposed
```

Each one drafts the request from the record's own facts, shows what is being
signed, and asks once. There is no flag to answer for you.

## Check the setup first

```bash
war admin doctor
```

At the time of writing it reports, for this repository:

| finding | meaning |
|---|---|
| `authority.unprotected` | `docs/authority/` and the policy keys are working-tree files the performer can write; review of them is the only control |
| `independence.undeclared` | §46.3's minimums cannot be evaluated for any Warrant |
| `sas.proposed-unaccepted` | the SAS is revision 0.2.1 (proposed); 0.1.0 remains normative until it is signed |
| `doctor.performer` / `doctor.verifier` | no CLI performer or verifier is configured, so `--run` is unavailable and the verifier must be a person or a separate agent context |
| `identity.changed` | the checkout is not a git repository to OpenWarrant's eye, so the manifest UUID cannot be compared |

None of these block the work. They decide how much the signature is worth, and
that is a judgement for the person signing.

## Where the evidence lives

- `docs/warrants/<alias>/gate-runs/` — receipts minted by `war evidence record`,
  bound to the contract digest they ran against. `war status` reads these and
  nothing else; the gitignored `docs/receipts/` scratch path is never read.
- `docs/warrants/<alias>/deliverables.toml` — the pinned digests. A closed
  Warrant's delivered file is pinned by digest: `war check` reports drift, and
  correcting one is `war sign correct <alias> <D-id>`, which a person signs.

## Honest state of this backlog

`war status` reads every Warrant below `would_satisfy` because no resolution has
been signed. That is not a defect to work around — it is the mechanism. The
work-order half of the goal is complete when `war sign resolve --dry-run` is
clean for each Warrant and this list is what remains for you:

1. `war admin doctor`, and decide what to do about the findings above.
2. `war sign inbox`, then walk it.
