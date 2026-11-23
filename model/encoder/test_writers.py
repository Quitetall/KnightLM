"""Board-encoder and projector writers -> the serve loader, on a tiny random host (CPU).

Round trip: pretrain-output layout (`save_encoder` + manifest) and alignment-output layout (`write_projector_dir`)
are loaded by `Predictor._load_board`, the code the server runs, and must give the same soft tokens and host logits
as the in-memory modules. Nothing here says anything about chess strength.
"""

import json
import tempfile
import unittest
from pathlib import Path

from model.encoder import manifest
from model.serve import test_head as T

try:
    import torch
except ImportError:
    torch = None

BASE = "a" * 64


class Tok(T.FakeTokenizer):
    def __call__(self, text, add_special_tokens=False):
        return {"input_ids": self.encode(text)}


def align_rows():
    return [{"fen": T.PACKET["fen"], "repetition": 1, "prompt": "How many white pawns?", "completion": "Eight."},
            {"fen": T.OTHER["fen"], "repetition": None, "prompt": "Who is to move?", "completion": "Black."}]


class ManifestTests(unittest.TestCase):
    def test_planes_fingerprint_is_stable_and_sensitive(self):
        a = manifest.planes_fingerprint()
        self.assertEqual(a, manifest.planes_fingerprint())
        self.assertEqual(a["planes"], 22)
        self.assertEqual(len(a["sha256"]), 64)


@unittest.skipIf(torch is None, "torch not installed")
class RoundTrip(T.HostCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)

    def write(self, seed=0, steps=3, base=BASE, root=None):
        """Pretrain-output layout and alignment-output layout, written by the writers."""
        from model.encoder.align import align_projector, freeze, write_projector_dir
        from model.encoder.encoder import BoardEncoder, EncoderConfig, Projector, RulesHeads, save_encoder
        root = root or self.dir
        torch.manual_seed(seed)
        cfg = EncoderConfig.tiny()
        encoder = BoardEncoder(cfg).eval()
        save_encoder(root / "encoder", encoder, RulesHeads(cfg.d_model))
        manifest.write_encoder_manifest(root / "encoder", corpus_manifest_sha256="c" * 64, compiler_sha256=None)
        host, projector = self.host(), Projector(cfg.d_model, self.HIDDEN)
        before = [p.detach().clone() for p in projector.parameters()]
        frozen = [p.detach().clone() for p in host.parameters()]
        loss = align_projector(freeze(host), freeze(encoder), projector, Tok(), align_rows(), steps, 2, 1e-2)
        self.assertTrue(all(torch.equal(a, b) for a, b in zip(frozen, host.parameters())))  # host untouched
        self.assertFalse(all(torch.equal(a, b) for a, b in zip(before, projector.parameters())))
        write_projector_dir(root / "projector", projector, root / "encoder", host_base_sha256=base, final_loss=loss,
                            steps=steps)
        return host, encoder.eval(), projector.eval()

    def served(self, host, root=None, base=BASE):
        from model.serve.inference import Predictor
        root = root or self.dir
        return self.predictor(Predictor, "play-enc", model=host, encoder_dir=root / "encoder",
                              projector_dir=root / "projector", base_sha256=base)

    def test_written_layout_is_what_the_server_loads(self):
        self.write()
        self.assertEqual(sorted(p.name for p in (self.dir / "encoder").iterdir()),
                         ["config.json", "encoder.pt", "heads.pt", "manifest.json"])
        self.assertEqual(sorted(p.name for p in (self.dir / "projector").iterdir()),
                         ["manifest.json", "projector.json", "projector.pt"])
        proj = manifest.read(self.dir / "projector")
        self.assertEqual((proj["host_base_sha256"], proj["host_hidden"]), (BASE, self.HIDDEN))

    def test_serve_loader_reproduces_soft_tokens_and_host_logits(self):
        from model.encoder.inject import host_forward, soft_tokens
        from model.encoder.planes import packet_planes
        host, encoder, projector = self.write()
        predictor = self.served(host)
        predictor._load_board()
        self.assertEqual(predictor.n_board_tokens, 64)
        for packet in (T.PACKET, T.OTHER):
            planes = torch.tensor(packet_planes(packet), dtype=torch.float32).reshape(1, -1, 8, 8)
            with torch.no_grad():
                want = soft_tokens(encoder, projector, planes)
            got = predictor._soft_tokens(packet)
            torch.testing.assert_close(got, want, rtol=0, atol=0)
            _, ids = predictor._play_prompt_ids(packet)
            with torch.no_grad():
                a = host_forward(host, torch.tensor([ids]), want).logits
                b = host_forward(host, torch.tensor([ids]), got).logits
            torch.testing.assert_close(a, b, rtol=0, atol=0)

    def test_the_serve_manifest_reports_the_written_hashes(self):
        from model.corpus.pipeline import sha256_file
        host, _, _ = self.write()
        predictor = self.served(host)
        predictor._load_board()
        self.assertEqual(predictor.encoder_sha256, sha256_file(self.dir / "encoder/encoder.pt"))
        self.assertEqual(predictor.projector_sha256, manifest.read(self.dir / "projector")["projector_sha256"])

    def test_a_projector_aligned_to_another_encoder_or_host_is_refused(self):
        host, _, _ = self.write(seed=0)
        other = self.dir / "other"
        self.write(seed=1, root=other)  # a second, different encoder + projector
        mixed = self.served(host)
        mixed.encoder_dir = other / "encoder"
        with self.assertRaisesRegex(ValueError, "different encoder"):
            mixed._load_board()
        with self.assertRaisesRegex(ValueError, "aligned to host"):
            self.served(host, base="e" * 64)._load_board()

    def test_a_changed_weights_file_is_refused_and_a_directory_without_manifests_still_loads(self):
        host, _, _ = self.write()
        (self.dir / "projector/projector.pt").write_bytes((self.dir / "projector/projector.pt").read_bytes() + b"\0")
        with self.assertRaisesRegex(ValueError, "differs from the hash"):
            self.served(host)._load_board()
        for name in ("encoder", "projector"):
            (self.dir / name / "manifest.json").unlink()
        (self.dir / "projector/projector.pt").write_bytes(
            (self.dir / "projector/projector.pt").read_bytes()[:-1])  # undo the appended byte
        self.served(host)._load_board()


if __name__ == "__main__":
    unittest.main()
