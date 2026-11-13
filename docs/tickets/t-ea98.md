---
format: openwarrant/1.0
profile: ticket
alias: t-ea98
uuid: 01a1b895-f40e-71ac-b3e3-f454e1d335d2
priority: 3
created_at: "2026-11-08T00:36:37Z"
created_by: "codex-contributor-setup"
summary: "Provide one reproducible training entry point that validates the supplied corpus and invokes the existing LoRA trainer."
labels:
  - "mlops-cv"
paths:
  - "model/train/**"
  - "model/configs/**"
  - "model/README.md"
x.depends_on:
  - "t-5cd8"
requires:
  - "x.depends_on"
---
# MLOps/CV Engineer — Make the trainer runnable from a corpus manifest

**Start after** the [corpus contract](t-5cd8.md). Use its tiny fixture
while Data builds the ingestion path. Start by running the existing
[training harness](../../model/train/harness.py) in its pilot configuration.

Build one documented entry point that accepts a corpus manifest, pinned base
model, explicit training configuration, and a new run output directory.
Preflight must work on CPU and report what would run. Starting GPU training
requires an explicit option. Pass the verified corpus to the existing
SmolVLM LoRA trainer and record the corpus digest, model revision, seed,
training settings, checkpoint, and completion or failure status. Keep
previous runs and allow a deliberate retry in a new output location.

**Review.** Demonstrate the valid and tampered fixtures in preflight, and
one bounded optimizer step with a reload on an agreed GPU. Capture the
exact command and run manifest. Give the pipeline tester a way to invoke
the entry point with a fixture on CPU.

## Checklist


- [ ] Document and implement the manifest-to-training entry point. (i-50f7)
- [ ] Show CPU preflight refusal and a bounded GPU checkpoint reload. (i-7e6d)
