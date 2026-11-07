# KnightLM implementation contract

KnightLM tests whether fresh complete state packets improve chess reasoning,
play and context cost. Rust/Burn is canonical. Python and Mojo start as protocol
parity slices. No arm is presumed to win.

The intended model is a fine-tuned multimodal Qwen 3.5-class 4B/9B model.
SmolVLM-500M-Instruct is the first small integration target, not the final capacity
target. The same model observes an image and then reasons in a separate fresh
pass from a deterministic packet. All KV, recurrent and convolution state must
reset between these passes and between fresh decisions.

The initial deliverable is a runnable CPU research scaffold, including state
replay, compilation, experiment fixtures, a Burn gradient/checkpoint smoke and
cross-language packet parity. It does not include pretrained inference, bulk
downloads, training campaigns, or claims of playing strength.

Game history belongs to the deterministic compiler, outside model memory. State
contains board, turn, rights, counters and compact repetition-relevant history.
Unknown history is not invented. Compiler facts distinguish claimable and
automatic draws. General dead-position adjudication is explicitly incomplete.
Observation/ledger disagreement is an explicit perception failure, never a
silent repair.

Eight initial arms: history; history plus current state; fresh current state;
state plus legal choices; state plus legal masking; state plus derived features;
state plus rule simulation; combined assistance. Shared checkpoint comparison
comes before arm-specific fine-tunes with matched budgets. Rule simulation has
an eight-call fixture budget. No strategic engine is accessible at inference.
Engine evaluation is allowed only for teacher data and post-decision scoring.

Lichess evaluation JSONL is position supervision, not a complete game ledger.
Normalized replay fixtures establish full state. PGN ingestion is a later stage.
Verified synthetic semantic questions come first. Expert/book annotations and
general-language retention corpora retain provenance. Related game, position,
counterfactual and visual variants must not leak between scored splits.

Score raw legality, assisted legality, unaided causal accuracy, explanations,
whole-board recognition, play strength, general-language retention and resource
cost separately. Tool calls, interventions, failures, overflows and abstentions
remain visible. Fixture reports prove wiring only.

Future pretrained admission: pinned artifacts and processor/tokenizer parity;
reference logits; finite nonzero gradients and updates; tiny-set overfit; reload
equivalence; reset isolation. SmolVLM admission does not admit Qwen.

Default scope: standard untimed chess, rendered/digital board images, CLI,
local CPU checks. Other domains retain a compiler seam but no transfer claim.
