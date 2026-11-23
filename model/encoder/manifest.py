"""Manifests that bind the board-encoder artifacts (torch-free), beside the files the server already loads.

Layouts (unchanged; `model.serve.inference.Predictor._load_board` reads them):

  encoder_dir/    encoder.pt  config.json  [heads.pt]  [manifest.json]      <- `pretrain` writes `<out>/encoder/`
  projector_dir/  projector.pt  projector.json  [manifest.json]             <- `align` writes `<out>/`

`manifest.json` records what each was made from: the planes fingerprint (the plane builder's output on fixed
FENs, so a refactor that keeps the planes does not invalidate an encoder, and one that changes them does), the
corpus and compiler the labels came from, and for the projector the encoder hash and the host base hash it was
aligned against. `check_pair` is what the server runs when a manifest is present: a projector aligned to another
encoder, or to another host, is refused by name. Directories without manifests still load.
"""

import hashlib
import json
from pathlib import Path

from model.corpus.pipeline import sha256_file

ENCODER_SCHEMA = "knightlm.board-encoder/v1"
PROJECTOR_SCHEMA = "knightlm.board-projector/v1"
TASK_SCHEMA = "knightlm.board-task/v1"
TASK_FILE = "task.json"
FILE = "manifest.json"
_FENS = ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
         "rnbqkbnr/ppp1pppp/8/8/3pP3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 3")


def planes_fingerprint():
    from model.encoder.planes import PLANES, build_planes
    digest = hashlib.sha256()
    for fen, rep in ((_FENS[0], None), (_FENS[0], 1), (_FENS[1], 2)):
        digest.update(json.dumps(build_planes(fen, rep)).encode())
    return {"planes": PLANES, "sha256": digest.hexdigest()}


def write(directory, manifest):
    path = Path(directory) / FILE
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return path


def read(directory):
    path = Path(directory) / FILE
    return json.loads(path.read_text()) if path.is_file() else None


def write_encoder_manifest(encoder_dir, *, corpus_manifest_sha256=None, compiler_sha256=None, axis_sha256=None,
                           train=None):
    """Written after `save_encoder`. Unknown inputs stay null."""
    encoder_dir = Path(encoder_dir)
    config = json.loads((encoder_dir / "config.json").read_text())
    return write(encoder_dir, {
        "schema": ENCODER_SCHEMA, "encoder_sha256": sha256_file(encoder_dir / "encoder.pt"), "config": config,
        "n_tokens": 64 + config.get("n_global", 0), "planes": planes_fingerprint(),
        "corpus_manifest_sha256": corpus_manifest_sha256, "compiler_sha256": compiler_sha256,
        "axis_sha256": axis_sha256, "train": train or {}})


def write_projector_manifest(projector_dir, encoder_dir, *, host_base_sha256, final_loss=None, steps=None,
                             rows_sha256=None, train=None):
    """Written after `save_projector`: binds the projector to the encoder file and the host base it was aligned on."""
    projector_dir = Path(projector_dir)
    meta = json.loads((projector_dir / "projector.json").read_text())
    return write(projector_dir, {
        "schema": PROJECTOR_SCHEMA, "projector_sha256": sha256_file(projector_dir / "projector.pt"),
        "encoder_sha256": sha256_file(Path(encoder_dir) / "encoder.pt"), "host_base_sha256": host_base_sha256,
        "d_encoder": meta["d_encoder"], "host_hidden": meta["host_hidden"], "planes": planes_fingerprint(),
        "final_loss": final_loss, "steps": steps, "rows_sha256": rows_sha256, "train": train or {}})


def check_pair(encoder_dir, projector_dir, base_sha256=None):
    """Mismatches between the manifests (when present), the files on disk and the host being served."""
    found = []
    enc, proj = read(encoder_dir), read(projector_dir)
    here = planes_fingerprint()
    if enc is not None:
        if enc.get("schema") != ENCODER_SCHEMA:
            found.append(f"encoder manifest schema {enc.get('schema')!r}")
        if enc.get("encoder_sha256") != sha256_file(Path(encoder_dir) / "encoder.pt"):
            found.append("encoder.pt differs from the hash in the encoder manifest")
        if enc.get("planes") != here:
            found.append("the encoder was trained on other input planes than this checkout builds")
    if proj is not None:
        if proj.get("schema") != PROJECTOR_SCHEMA:
            found.append(f"projector manifest schema {proj.get('schema')!r}")
        if proj.get("projector_sha256") != sha256_file(Path(projector_dir) / "projector.pt"):
            found.append("projector.pt differs from the hash in the projector manifest")
        if proj.get("encoder_sha256") != sha256_file(Path(encoder_dir) / "encoder.pt"):
            found.append("the projector was aligned to a different encoder")
        if proj.get("planes") != here:
            found.append("the projector was aligned on other input planes than this checkout builds")
        if base_sha256 is not None and proj.get("host_base_sha256") != base_sha256:
            found.append(f"the projector was aligned to host {proj.get('host_base_sha256')}, serving {base_sha256}")
    return found


def write_task_manifest(run_dir, *, base_sha256, adapter_sha256, encoder_sha256, projector_sha256, prompt_kind,
                        n_board_tokens, rows_sha256=None, corpus_manifest_sha256=None, compiler_sha256=None,
                        train=None):
    """`<run>/task.json`: the adapter and the projector that were trained together (T4/T5), and what they read.
    `adapter_sha256` is null for the projector-only stage. The server checks it when it loads the adapter
    from `<run>/adapter` (`check_task`)."""
    return write_at(Path(run_dir) / TASK_FILE, {
        "schema": TASK_SCHEMA, "base_sha256": base_sha256, "adapter_sha256": adapter_sha256,
        "encoder_sha256": encoder_sha256, "projector_sha256": projector_sha256, "prompt_kind": prompt_kind,
        "n_board_tokens": n_board_tokens, "planes": planes_fingerprint(), "rows_sha256": rows_sha256,
        "corpus_manifest_sha256": corpus_manifest_sha256, "compiler_sha256": compiler_sha256, "train": train or {}})


def write_at(path, manifest):
    path = Path(path)
    path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return path


def read_task(run_dir):
    path = Path(run_dir) / TASK_FILE
    return json.loads(path.read_text()) if path.is_file() else None


def check_task(task, *, base_sha256=None, adapter_sha256=None, encoder_sha256=None, projector_sha256=None,
               prompt_kind=None):
    """Mismatches between a task manifest and what is being served: an adapter trained with another projector,
    encoder or base is refused by name. Absent arguments are not checked."""
    found = []
    if task.get("schema") != TASK_SCHEMA:
        found.append(f"task manifest schema {task.get('schema')!r}")
    for name, got in (("base_sha256", base_sha256), ("adapter_sha256", adapter_sha256),
                      ("encoder_sha256", encoder_sha256), ("projector_sha256", projector_sha256)):
        if got is not None and task.get(name) != got:
            found.append(f"the adapter was trained with {name[:-7]} {task.get(name)}, serving {got}")
    if prompt_kind is not None and task.get("prompt_kind") != prompt_kind:
        found.append(f"the adapter was trained on prompt kind {task.get('prompt_kind')!r}, serving {prompt_kind!r}")
    if task.get("planes") != planes_fingerprint():
        found.append("the adapter was trained on other input planes than this checkout builds")
    return found
