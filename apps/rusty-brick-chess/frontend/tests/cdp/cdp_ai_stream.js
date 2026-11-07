// Streaming, stop and tool cards — self-contained.
//
// This script serves its own OpenAI-compatible SSE provider, so the check does
// not depend on an external mock staying alive. The app's server must be
// pointed at it: MODEL_URL=http://127.0.0.1:8100/v1/chat.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9536;
const PROVIDER = 8100;
const CHUNK_DELAY_MS = 400;

// --- a provider that streams slowly and can call a tool --------------------

function startProvider() {
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      let payload = {};
      try { payload = JSON.parse(body || "{}"); } catch {}
      const messages = payload.messages || [];
      const sawTool = messages.some((message) => message.role === "tool");
      const user = ([...messages].reverse().find((message) => message.role === "user") || {}).content || "";
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      });
      const send = (obj) => res.write("data: " + JSON.stringify(obj) + "\n\n");
      const chunk = (delta) => send({ id: "mock", choices: [{ index: 0, delta }] });
      const finish = (tokensIn, tokensOut) => {
        send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "stop" }] });
        send({ id: "mock", choices: [], usage: { prompt_tokens: tokensIn, completion_tokens: tokensOut } });
        res.write("data: [DONE]\n\n");
        res.end();
      };

      if (/board/i.test(user) && !sawTool) {
        // A tool round: ask for the boards, the server answers, we reply.
        chunk({ tool_calls: [{ index: 0, id: "call_1", function: { name: "list_boards", arguments: "{}" } }] });
        send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] });
        send({ id: "mock", choices: [], usage: { prompt_tokens: 5, completion_tokens: 2 } });
        res.write("data: [DONE]\n\n");
        res.end();
        return;
      }

      if (user === "stream please") {
        // Continue streaming until Stop so the browser has time to observe a
        // partial response. The provider checks cancellation between reads.
        chunk({ content: "the position is " });
        const interval = setInterval(() => chunk({ content: "quiet " }), CHUNK_DELAY_MS);
        res.on("close", () => clearInterval(interval));
        return;
      }

      const words = (sawTool ? "there are boards here" : "the position is equal and quiet").split(" ");
      let index = 0;
      const tick = () => {
        if (index < words.length) {
          chunk({ content: words[index++] + " " });
          setTimeout(tick, CHUNK_DELAY_MS);
          return;
        }
        finish(sawTool ? 11 : 9, sawTool ? 4 : 6);
      };
      tick();
    });
  });
  server.listen(PROVIDER, "127.0.0.1");
  return server;
}

function getJson(port, path) {
  return new Promise((resolve, reject) => {
    http.get({ host: "127.0.0.1", port, path }, (response) => {
      let data = "";
      response.on("data", (chunk) => (data += chunk));
      response.on("end", () => resolve(JSON.parse(data)));
    }).on("error", reject);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

(async () => {
  const provider = startProvider();
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-stream2-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let target;
  for (let i = 0; i < 40; i++) { await sleep(300); try { target = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(target.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (method, params = {}) => new Promise((resolve) => { const i = ++id; pend.set(i, resolve); ws.send(JSON.stringify({ id: i, method, params })); });
  ws.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((resolve) => (ws.onopen = resolve));
  await send("Runtime.enable");
  await sleep(2800);
  const ev = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (selector, text) => await ev(`(()=>{const e=[...document.querySelectorAll(${JSON.stringify(selector)})].find(x=>x.textContent.trim()===${JSON.stringify(text)});e&&e.click();return !!e;})()`);
  const type = async (text) => await ev(`(()=>{const t=document.querySelector(".chat-input textarea");if(!t)return false;t.value=${JSON.stringify(text)};t.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  const streamingText = async () => await ev(`(()=>{const b=document.querySelector(".chat-msg.streaming");return b?b.textContent.trim():"";})()`);
  const waitFor = async (read, ms = 8000) => { const start = Date.now(); while (Date.now() - start < ms) { const value = await read(); if (value) return value; await sleep(250); } return ""; };

  await clickText(".nav", "Assistant");
  await sleep(900);
  await clickText("button", "New chat");
  await sleep(1500);

  // --- streaming: text shows up before the turn ends ----------------------
  await type("stream please");
  await sleep(200);
  const started = await clickText(".chat-input button", "Send");
  const partial = await waitFor(streamingText, 12000);
  const labelWhileSending = await ev(`document.querySelector(".chat-input button")?.textContent.trim()`);

  // --- stop: the turn ends by request ------------------------------------
  const stopped = await clickText(".chat-input button", "Stop");
  const errorText = await waitFor(async () => await ev(`(()=>{const e=document.querySelector(".chat-msg.error");return e?e.textContent.trim():"";})()`), 8000);
  const labelAfterStop = await ev(`document.querySelector(".chat-input button")?.textContent.trim()`);

  // --- tool cards and usage: a stored turn with a tool result ------------
  await sleep(1000);
  await type("which boards do I have");
  await sleep(200);
  await clickText(".chat-input button", "Send");
  const cards = await waitFor(async () => await ev(`document.querySelectorAll(".tool-card").length`), 15000);
  const usage = await waitFor(async () => await ev(`[...document.querySelectorAll(".muted")].some(e=>/\\bin\\b/.test(e.textContent)&&/\\bout\\b/.test(e.textContent))`), 8000);

  const out = { started, partial: String(partial).slice(0, 40), labelWhileSending, stopped, errorText, labelAfterStop, cards, usage };
  console.log(JSON.stringify(out));
  const ok = started && String(partial).length > 0 && labelWhileSending === "Stop"
    && stopped && /cancel/i.test(String(errorText)) && labelAfterStop === "Send"
    && Number(cards) > 0 && usage === true;
  console.log(ok ? "AI_STREAM_PASS" : "AI_STREAM_FAIL");
  ws.close();
  chrome.kill();
  provider.close();
  process.exit(ok ? 0 : 1);
})().catch((error) => { console.error(error); process.exit(1); });
