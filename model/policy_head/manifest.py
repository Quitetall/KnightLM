"""The manifest that binds a trained move head to what it was trained with (torch-free).

`manifest.json` sits beside `head.pt` and `head.json` in the head directory
(`model.policy_head.head.save_head`). It binds the head to the base weights, the adapter it was
trained jointly with, the ctok/1 revision (move space) and the rules the labels came from, so a head
cannot be served on another adapter without the mismatch being named. `head.pt`/`head.json` keep
their layout; a head directory without a manifest still loads (older heads), and
`model.serve.head.HeadPredictor.attach_head` checks the manifest when there is one.
"""

import json
from pathlib import Path

from model.policy_head import space

SCHEMA = "knightlm.move-head/v1"
LABEL = "human-only move head"
FILE = "manifest.json"
PROMPT_KIND = "play-head"
RULESET = "standard-untimed-v1"


def ctok_revision():
    from model.tokenizer import vocab
    return {"spec_version": vocab.SPEC_VERSION, "vocab_sha256": vocab.vocab_sha256(), "moves": space.SIZE}


def rules_revision(compiler_sha256=None):
    """The rules the legal masks come from: the packet ruleset and (when known) the compiler binary.
    An unknown compiler stays null, never a made-up hash."""
    from model import prompt
    return {"ruleset": RULESET, "prompt_kind": PROMPT_KIND, "head_label": prompt.HEAD_LABEL,
            "compiler_sha256": compiler_sha256}


def build(*, head_sha256, hidden_size, base_sha256, adapter_sha256, corpus_manifest_sha256=None,
          compiler_sha256=None, train=None):
    """`adapter_sha256` is None for a head-only (frozen host, no adapter) probe."""
    return {"schema": SCHEMA, "label": LABEL, "head_sha256": head_sha256, "hidden_size": hidden_size,
            "base_sha256": base_sha256, "adapter_sha256": adapter_sha256 or "none",
            "ctok": ctok_revision(), "rules": rules_revision(compiler_sha256),
            "corpus_manifest_sha256": corpus_manifest_sha256, "train": train or {}}


def write(head_dir, manifest):
    path = Path(head_dir) / FILE
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return path


def read(head_dir):
    """The manifest, or None when the directory has none (a head saved by `save_head` alone)."""
    path = Path(head_dir) / FILE
    return json.loads(path.read_text()) if path.is_file() else None


def problems(manifest, *, head_sha256=None, hidden_size=None, base_sha256=None, adapter_sha256=None):
    """What does not match. A value passed as None is not checked (the caller does not know it); "none" is the adapter-less value."""
    found = []
    if manifest.get("schema") != SCHEMA:
        found.append(f"manifest schema {manifest.get('schema')!r} is not {SCHEMA!r}")
    ctok = ctok_revision()
    if manifest.get("ctok") != ctok:
        found.append(f"head was trained on ctok {manifest.get('ctok')}; this checkout is {ctok}")
    if manifest.get("rules", {}).get("prompt_kind") != PROMPT_KIND:
        found.append(f"head was trained on prompt kind {manifest.get('rules', {}).get('prompt_kind')!r}")
    for name, want, have in (("head_sha256", head_sha256, manifest.get("head_sha256")),
                             ("hidden_size", hidden_size, manifest.get("hidden_size")),
                             ("base_sha256", base_sha256, manifest.get("base_sha256")),
                             ("adapter_sha256", adapter_sha256, manifest.get("adapter_sha256"))):
        if want is not None and want != have:
            found.append(f"{name}: serving {want}, head manifest binds {have}")
    return found


def check(head_dir, **kwargs):
    """Read and check the manifest of `head_dir`; raises ValueError naming every mismatch. Returns it (or None)."""
    manifest = read(head_dir)
    if manifest is None:
        return None
    found = problems(manifest, **kwargs)
    if found:
        raise ValueError("move head does not match what is being served: " + "; ".join(found))
    return manifest
