"""S0 rules-QA axis: exact-answer match per question type (docs: model/corpus/s0.py).

The model answers one question about a position in the play-call format; the answer is the
`reasoning` field of its submit_move call and is compared with the compiler-derived answer
character for character. A reply that does not parse as a play call, or has no answer, is a
failure on that row (it stays in the denominator). The claim-failure rate of a type is
1 - exact rate. `explain.py` also checks the answer text where its claim types apply (piece on
square, square state, legal moves named); that block is reported beside, never merged.
There is no composite score and no engine.

The edge questions (`attackers_of`, `defenders_of`) ask about cgt/1 atk/def edges, which only a graph
arm (T2 cgt-lite shows the edges only through its `atk`/`def`-free node list, T2-full prints them) can
read off the prompt. They are reported as their own group, `groups.edge`, beside `groups.node` (every
other type); `overall` still counts all rows. Compare arms on a group with the edge-printing arm
(`play-cgt-full`) for the edge group, and on `groups.node` across all arms.
"""

from model import prompt as prompt_module
from model.eval import explain as explain_module
from model.eval.scorecard import call_with_timeout, rate
from model.corpus.s0 import EDGE_TYPES, TYPES


class _Complete:
    """Adapter so call_with_timeout (which calls predict) can drive predictor.complete."""

    def __init__(self, predictor):
        self.predictor = predictor

    def predict(self, prompt_text, _mode):
        return self.predictor.complete(prompt_text)


def predict_rows(rows, predictor, kind, timeout_s=300.0):
    if not callable(getattr(predictor, "complete", None)):
        raise ValueError("an S0 evaluation needs a predictor with complete(prompt_text) (model.serve.inference.Predictor)")
    records = []
    adapter = _Complete(predictor)
    for index, row in enumerate(rows):
        packet, qa = row["packet"], row["qa"]
        served = prompt_module.qa_prompt(kind, packet, qa["question"])
        if served != row["prompt"]:
            raise ValueError(f"row {row['id']}: stored prompt differs from model.prompt ({kind})")
        value, err_kind, err, wall = call_with_timeout(adapter, served, None, timeout_s)
        record = {"idx": index, "id": row["id"], "type": qa["type"], "fen": packet["fen"], "answer": qa["answer"],
                  "raw_reply": None, "reasoning": None, "code": None, "exact": False, "explain": None,
                  "input_tokens": None, "output_tokens": None, "latency_ms": round(wall, 3), "error": err}
        if value is not None:
            reply, input_tokens, output_tokens, latency = value
            record.update(raw_reply=reply, input_tokens=input_tokens, output_tokens=output_tokens,
                          latency_ms=round(latency, 3))
            try:
                parsed = prompt_module.parse_play_completion(reply)
            except prompt_module.PlayCompletionError as error:
                record["code"] = error.code
            else:
                record["code"] = "ok"
                record["reasoning"] = parsed["reasoning"]
                record["exact"] = parsed["reasoning"].strip() == qa["answer"]
                record["explain"] = explain_module.check_explanation(
                    packet["fen"], packet["legal_moves"], parsed["reasoning"], None)
        else:
            record["code"] = err_kind
        records.append(record)
    return records


def block(records):
    n = len(records)
    exact = sum(r["exact"] for r in records)
    parsed = sum(r["code"] == "ok" for r in records)
    return {"n": n, "exact": exact, "rate": rate(exact, n), "claim_failure_rate": rate(n - exact, n),
            "parse_failures": n - parsed}


def axis(records):
    by_type = {t: block([r for r in records if r["type"] == t]) for t in TYPES
               if any(r["type"] == t for r in records)}
    explained = [r["explain"] for r in records]
    groups = {"node": block([r for r in records if r["type"] not in EDGE_TYPES]),
              "edge": block([r for r in records if r["type"] in EDGE_TYPES])}
    return {"overall": block(records), "groups": groups, "edge_types": list(EDGE_TYPES), "by_type": by_type,
            "explain_checks": explain_module.aggregate(explained) | {"source": "answer text, where explain.py claim types apply"}}


def scorecard(records, kind, n_rows):
    if len(records) < n_rows:
        raise ValueError(f"{n_rows - len(records)} requested rows have no record")
    tokens = [r["input_tokens"] for r in records if r["input_tokens"] is not None]
    return {"schema": "knightlm.scorecard/s0", "composite_score": None,
            "composite_note": "none by design: the S0 axis is reported per question type",
            "prompt_kind": kind, "modes": ["raw"],
            "axes": {"rows": len(records), "s0_rules_qa": axis(records),
                     "prompt_tokens": {"mean": round(sum(tokens) / len(tokens), 1) if tokens else None, "n": len(tokens)}}}
