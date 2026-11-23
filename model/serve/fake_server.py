"""The real serving app (model.serve.server.create_app) over a fake predictor, for game gates.

The predictor answers every packet with its first legal move, so it is always legal and
never needs a model, torch or a GPU. What it exercises is everything around the model:
the HTTP contract, the packet format, `knightlm-uci`, and the match driver.

    python3 -m model.serve.fake_server --port 8123
"""
import argparse
import tempfile
from pathlib import Path

from model.prompt import format_play_completion
from model.serve import play
from model.serve.inference import CoachGeneration, PlayPrediction, Prediction


class FirstLegalPredictor:
    base_dir = None
    base_sha256 = "f" * 64
    adapter_sha256 = None

    def manifest(self):
        return {"base_sha256": self.base_sha256, "adapter_sha256": "none", "prompt_sha256": "fake",
                "tokenizer_sha256": "fake", "chat_template_sha256": "fake"}

    def __init__(self, illegal=False):
        self.illegal = illegal  # for the gate's refusal check: answer a move that is never legal
        self.prompts = []  # the coach prompts rendered for this predictor, for the wire tests
        self.legal = []  # the packet's legal moves, set by build_app for a coach turn (the prompt hides them)

    def predict(self, packet, mode):
        if self.illegal:
            return Prediction("a1a1", "a1a1", False, 1, 1, 0.0)
        move = packet["legal_moves"][0]
        return Prediction(move, move, True, 1, 1, 0.0)


    def predict_play(self, packet, mode):
        """Play mode (what /v1/chat/completions and /v1/play call): a well-formed call for the first legal move."""
        move = "a1a1" if self.illegal else packet["legal_moves"][0]
        reply = format_play_completion(f"Fake predictor: the first legal move is {move}.", move)
        fields, extra = play.resolve(reply, packet["legal_moves"], mode, lambda prefix, suffix: (move, 1))
        return PlayPrediction(raw_reply=reply, input_tokens=1, output_tokens=1 + extra, latency_ms=0.0, **fields)


    def predict_coach(self, prompt_text):
        """Coach mode (chat on): a `Provisional:` line for the first legal move, a reply and one board mark.
        The coach prompt never shows the legal list, so build_app hands it over through `self.legal`."""
        self.prompts.append(prompt_text)
        move = "a1a1" if self.illegal else self.legal[0]
        text = (f"Provisional: {move}\nFake coach: the first legal move is {move}.\n"
                '<tool_call>{"name": "mark_square", "arguments": {"square": "e4", "color": "blue"}}</tool_call>')
        return CoachGeneration(text, 1, 1)


def build_app(workdir, illegal=False):
    import model.serve.server as server
    (Path(workdir) / "tokenizer.json").write_text("{}")
    (Path(workdir) / "chat_template.json").write_text("{}")
    predictor = FirstLegalPredictor(illegal)
    predictor.base_dir = Path(workdir)
    play_coach_turn = server.coach_mode.play_coach_turn

    def with_legal(predictor_, packet, *args, **kwargs):
        predictor_.legal = packet["legal_moves"]
        return play_coach_turn(predictor_, packet, *args, **kwargs)

    server.coach_mode.play_coach_turn = with_legal
    app = server.create_app(predictor, "raw")
    app.state.predictor = predictor
    return app


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
