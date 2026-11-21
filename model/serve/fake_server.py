"""The real serving app (model.serve.server.create_app) over a fake predictor, for game gates.

The predictor answers every packet with its first legal move, so it is always legal and
never needs a model, torch or a GPU. What it exercises is everything around the model:
the HTTP contract, the packet format, `knightlm-uci`, and the match driver.

    python3 -m model.serve.fake_server --port 8123
"""
import argparse
import tempfile
from pathlib import Path

from model.serve.inference import Prediction


class FirstLegalPredictor:
    base_dir = None
    base_sha256 = "f" * 64
    adapter_sha256 = None

    def manifest(self):
        return {"base_sha256": self.base_sha256, "adapter_sha256": "none", "prompt_sha256": "fake",
                "tokenizer_sha256": "fake", "chat_template_sha256": "fake"}

    def __init__(self, illegal=False):
        self.illegal = illegal  # for the gate's refusal check: answer a move that is never legal

    def predict(self, packet, mode):
        if self.illegal:
            return Prediction("a1a1", "a1a1", False, 1, 1, 0.0)
        move = packet["legal_moves"][0]
        return Prediction(move, move, True, 1, 1, 0.0)


def build_app(workdir, illegal=False):
    import model.serve.server as server
    (Path(workdir) / "tokenizer.json").write_text("{}")
    (Path(workdir) / "chat_template.json").write_text("{}")
    predictor = FirstLegalPredictor(illegal)
    predictor.base_dir = Path(workdir)
    return server.create_app(predictor, "raw")


def main():
    import uvicorn
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--illegal", action="store_true", help="always answer an illegal move")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        uvicorn.run(build_app(tmp, args.illegal), host="127.0.0.1", port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
