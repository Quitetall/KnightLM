"""OpenAI-compatible endpoint checks (CPU only): shapes, tool fencing, streaming, the official client."""

import json
import tempfile
import unittest
from pathlib import Path

from model.prompt import play_text
from model.serve.test_play import PACKET, SAY, ServerTests, good

BODY = {"model": "knightlm", "messages": [{"role": "user", "content": play_text(PACKET)}]}
PLAY_TOOL = {"type": "function", "function": {"name": "submit_move", "parameters": {"type": "object"}}}


def app_client(case, text, tmp, **kw):
    return ServerTests.client(case, text, tmp, **kw)


class ChatTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def client(self, text, **kw):
        return app_client(self, text, self.tmp.name, **kw)[0]

    def test_completion_shape_is_a_tool_call(self):
        r = self.client(good(), mode="constrained").post("/v1/chat/completions", json=dict(BODY, tools=[PLAY_TOOL]))
        self.assertEqual(r.status_code, 200)
        body = r.json()
        self.assertEqual(body["object"], "chat.completion")
        self.assertTrue(body["id"].startswith("chatcmpl-"))
        choice = body["choices"][0]
        self.assertEqual((choice["index"], choice["finish_reason"]), (0, "tool_calls"))
        message = choice["message"]
        self.assertEqual((message["role"], message["content"]), ("assistant", None))
        call = message["tool_calls"][0]
        self.assertEqual((call["type"], call["function"]["name"]), ("function", "submit_move"))
        self.assertEqual(json.loads(call["function"]["arguments"]), {"reasoning": SAY, "move": "e2e4"})
        self.assertEqual(list(json.loads(call["function"]["arguments"])), ["reasoning", "move"])
        self.assertEqual(body["usage"], {"prompt_tokens": 9, "completion_tokens": 5, "total_tokens": 14})
        self.assertEqual((body["knightlm"]["path"], body["knightlm"]["code"], body["knightlm"]["legal"]),
                         ("model", "ok", True))

    def test_constrained_fallback_is_labelled(self):
        from model.serve.test_play import good as g
        r = self.client(g(move="e2e5", reasoning="Advance."), mode="constrained").post("/v1/chat/completions", json=BODY)
        body = r.json()
        args = json.loads(body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"])
        self.assertEqual(args["move"], "g1f3")
        self.assertEqual((body["knightlm"]["path"], body["knightlm"]["raw_code"]), ("constrained_move", "illegal_move"))

    def test_raw_is_selectable_and_refusal_is_a_labelled_200(self):
        client = self.client("nonsense", mode="constrained")
        for r in (client.post("/v1/chat/completions?mode=raw", json=BODY),
                  client.post("/v1/chat/completions", json=dict(BODY, knightlm={"mode": "raw"}))):
            self.assertEqual(r.status_code, 200)
            body = r.json()
            self.assertEqual((body["choices"][0]["finish_reason"], body["choices"][0]["message"]["content"]),
                             ("stop", "nonsense"))
            self.assertNotIn("tool_calls", body["choices"][0]["message"])
            self.assertEqual((body["knightlm"]["code"], body["knightlm"]["legal"]), ("not_one_call", False))
        r = self.client("nonsense", mode="raw").post("/v1/chat/completions", json=BODY)
        self.assertEqual((r.status_code, r.json()["error"]["code"]), (502, "not_one_call"))

    def test_packet_forms(self):
        client = self.client(good(), mode="constrained")
        for content in (json.dumps(PACKET), json.dumps({"packet": PACKET}),
                        [{"type": "text", "text": play_text(PACKET)}]):
            body = dict(BODY, messages=[{"role": "system", "content": "x"}, {"role": "user", "content": content}])
            self.assertEqual(client.post("/v1/chat/completions", json=body).status_code, 200)
        for messages, status in (([{"role": "user", "content": "hello"}], 400),
                                 ([{"role": "assistant", "content": "x"}], 400),
                                 ([{"role": "user", "content": json.dumps(dict(PACKET, legal_moves=[]))}], 422)):
            r = client.post("/v1/chat/completions", json=dict(BODY, messages=messages))
            self.assertEqual(r.status_code, status)
            self.assertIn("error", r.json())

    def test_app_and_coach_tools_are_rejected_in_play_mode(self):
        client = self.client(good(), mode="constrained")
        for name in ("analyze", "evaluate", "play_move", "propose_provisional"):
            tool = {"type": "function", "function": {"name": name}}
            r = client.post("/v1/chat/completions", json=dict(BODY, tools=[PLAY_TOOL, tool]))
            self.assertEqual(r.status_code, 400)
            err = r.json()["error"]
            self.assertEqual((err["type"], err["code"], err["param"]), ("invalid_request_error", "tool_not_allowed",
                                                                        "tools[1]"))
        r = client.post("/v1/chat/completions", json=dict(BODY, tool_choice={"type": "function",
                                                                            "function": {"name": "analyze"}}))
        self.assertEqual(r.status_code, 400)
        ok = dict(BODY, tool_choice={"type": "function", "function": {"name": "submit_move"}})
        self.assertEqual(client.post("/v1/chat/completions", json=ok).status_code, 200)

    def test_unknown_model_and_models_list(self):
        client = self.client(good(), mode="constrained")
        r = client.post("/v1/chat/completions", json=dict(BODY, model="gpt-4"))
        self.assertEqual((r.status_code, r.json()["error"]["code"]), (404, "model_not_found"))
        listing = client.get("/v1/models").json()
        self.assertEqual((listing["object"], [m["id"] for m in listing["data"]]), ("list", ["knightlm"]))

    def test_stream_chunks_reassemble_to_the_completion(self):
        client = self.client(good(), mode="constrained")
        r = client.post("/v1/chat/completions", json=dict(BODY, stream=True, stream_options={"include_usage": True}))
        self.assertEqual(r.headers["content-type"].split(";")[0], "text/event-stream")
        lines = [l for l in r.text.split("\n\n") if l]
        self.assertEqual(lines[-1], "data: [DONE]")
        chunks = [json.loads(l[len("data: "):]) for l in lines[:-1]]
        self.assertTrue(all(c["object"] == "chat.completion.chunk" for c in chunks))
        self.assertEqual(chunks[0]["choices"][0]["delta"]["role"], "assistant")
        calls = [c["choices"][0]["delta"]["tool_calls"][0] for c in chunks if c["choices"] and "tool_calls" in c["choices"][0]["delta"]]
        self.assertEqual((calls[0]["function"]["name"], calls[0]["type"]), ("submit_move", "function"))
        args = "".join(c["function"]["arguments"] for c in calls)
        self.assertEqual(json.loads(args), {"reasoning": SAY, "move": "e2e4"})
        finals = [c for c in chunks if c["choices"] and c["choices"][0]["finish_reason"]]
        self.assertEqual((len(finals), finals[0]["choices"][0]["finish_reason"], finals[0]["knightlm"]["code"]),
                         (1, "tool_calls", "ok"))
        self.assertEqual(chunks[-1]["usage"]["total_tokens"], 14)

    def test_response_keys_match_the_published_example(self):
        """crates/contract/schemas/chat_completion_example.json is the wire example Rust parses."""
        example = json.loads((Path(__file__).resolve().parents[2] / "crates" / "contract" / "schemas"
                              / "chat_completion_example.json").read_text())
        body = self.client(good(), mode="constrained").post("/v1/chat/completions", json=BODY).json()

        def shape(v):
            if isinstance(v, dict):
                return {k: shape(x) for k, x in v.items()}
            if isinstance(v, list):
                return [shape(x) for x in v]
            return type(v).__name__

        self.assertEqual(shape(body), shape(example))
        self.assertEqual(body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
                         example["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"])

    def test_official_openai_client(self):
        try:
            import openai
        except ImportError:
            self.skipTest("openai package not installed in this environment")
        from fastapi.testclient import TestClient  # noqa: F401  (an httpx.Client)
        http = self.client(good(), mode="constrained")
        c = openai.OpenAI(base_url="http://testserver/v1", api_key="none", http_client=http)
        self.assertEqual([m.id for m in c.models.list().data], ["knightlm"])
        done = c.chat.completions.create(model="knightlm", messages=BODY["messages"], tools=[PLAY_TOOL],
                                         tool_choice="required", temperature=0, max_tokens=300)
        self.assertEqual(done.choices[0].finish_reason, "tool_calls")
        call = done.choices[0].message.tool_calls[0]
        self.assertEqual((call.function.name, json.loads(call.function.arguments)["move"]), ("submit_move", "e2e4"))
        self.assertEqual(done.usage.total_tokens, 14)
        with c.chat.completions.stream(model="knightlm", messages=BODY["messages"], tools=[PLAY_TOOL]) as s:
            final = s.get_final_completion()
        self.assertEqual(json.loads(final.choices[0].message.tool_calls[0].function.arguments)["move"], "e2e4")
        with self.assertRaises(openai.BadRequestError):
            c.chat.completions.create(model="knightlm", messages=BODY["messages"],
                                      tools=[{"type": "function", "function": {"name": "analyze"}}])


if __name__ == "__main__":
    unittest.main()
