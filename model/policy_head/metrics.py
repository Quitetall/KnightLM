"""Torch-free metrics over one row's logits, shared by tests and model.eval.move_head.

A row is (logits: sequence of SIZE floats, legal: iterable of class indices, target: class index).
Masked distribution = softmax over legal logits only (the product); raw = softmax over all SIZE.
"""

import math

from model.policy_head import space


def softmax(values):
    top = max(values)
    exps = [math.exp(v - top) for v in values]
    total = sum(exps)
    return [e / total for e in exps]


def masked_probs(logits, legal):
    """{class index: probability} over the legal set only."""
    legal = sorted(legal)
    return dict(zip(legal, softmax([logits[i] for i in legal])))


def raw_argmax(logits):
    return max(range(len(logits)), key=lambda i: (logits[i], -i))


def ranked_legal(logits, legal):
    return sorted(legal, key=lambda i: (-logits[i], i))


def row_stats(logits, legal, target):
    if len(logits) != space.SIZE:
        raise ValueError(f"logits must have {space.SIZE} entries, got {len(logits)}")
    legal = set(legal)
    ranked = ranked_legal(logits, legal)
    probs = masked_probs(logits, legal)
    raw = softmax(list(logits))
    raw_top = raw_argmax(logits)
    return {
        "masked_top1": ranked[0], "masked_top1_hit": ranked[0] == target,
        "masked_top3_hit": target in ranked[:3],
        "masked_target_prob": probs[target], "masked_top1_conf": probs[ranked[0]],
        "nll": -math.log(max(probs[target], 1e-12)),
        "raw_top1": raw_top, "raw_top1_legal": raw_top in legal, "raw_top1_hit": raw_top == target,
        "raw_illegal_mass": sum(p for i, p in enumerate(raw) if i not in legal),
    }


def calibration(confidences, hits, bins=10):
    """Expected calibration error over equal-width confidence bins, with the bins themselves."""
    n = len(confidences)
    table = []
    ece = 0.0
    for b in range(bins):
        lo, hi = b / bins, (b + 1) / bins
        idx = [i for i, c in enumerate(confidences) if (lo <= c < hi) or (b == bins - 1 and c == 1.0)]
        if not idx:
            continue
        conf = sum(confidences[i] for i in idx) / len(idx)
        acc = sum(hits[i] for i in idx) / len(idx)
        table.append({"lo": lo, "hi": hi, "n": len(idx), "confidence": round(conf, 4), "accuracy": round(acc, 4)})
        ece += len(idx) / n * abs(conf - acc)
    return {"ece": round(ece, 4) if n else None, "bins": table, "n": n}


def aggregate(stats):
    n = len(stats)
    if not n:
        return {"n": 0}
    mean = lambda key: round(sum(float(s[key]) for s in stats) / n, 4)
    return {"n": n, "masked_top1": mean("masked_top1_hit"), "masked_top3": mean("masked_top3_hit"),
            "masked_nll": mean("nll"), "raw_top1_legal": mean("raw_top1_legal"), "raw_top1_match": mean("raw_top1_hit"),
            "raw_illegal_mass_mean": mean("raw_illegal_mass"),
            "calibration": calibration([s["masked_top1_conf"] for s in stats], [s["masked_top1_hit"] for s in stats])}
