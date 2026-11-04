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

      if (/play/i.test(user)) {
        const toolMessage = messages.filter((m) => m.role === "tool").pop();
        if (!toolMessage) {
          // First round: find a board to write to.
          chunk({ tool_calls: [{ index: 0, id: "call_1", function: { name: "list_boards", arguments: "{}" } }] });
          send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] });
          send({ id: "mock", choices: [], usage: { prompt_tokens: 5, completion_tokens: 2 } });
          res.write("data: [DONE]\n\n");
          res.end();
          return;
        }
        // Second round: the tool result names a board, so ask to write to it.
        const match = /"board_id"\s*:\s*"([^"]+)"/.exec(String(toolMessage.content || ""));
        if (match) {
          chunk({ tool_calls: [{ index: 0, id: "call_2", function: { name: "play_move", arguments: JSON.stringify({ board_id: match[1], move: "e4", expected_revision: 1 }) } }] });
          send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] });
          send({ id: "mock", choices: [], usage: { prompt_tokens: 7, completion_tokens: 3 } });
          res.write("data: [DONE]\n\n");
          res.end();
          return;
        }
        finish(9, 4);
        return;
      }
      if (/board/i.test(user) && !sawTool) {
        // A tool round: ask for the boards, the server answers, we reply.
        chunk({ tool_calls: [{ index: 0, id: "call_1", function: { name: "list_boards", arguments: "{}" } }] });
        send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] });
        send({ id: "mock", choices: [], usage: { prompt_tokens: 5, completion_tokens: 2 } });
        res.write("data: [DONE]\n\n");
        res.end();
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

  const board = await ev(`(async()=>{
    const ws = await fetch('/api/workspaces',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({title:'Scopes',kind:'analysis'})}).then(r=>r.json());
    const b = await fetch('/api/boards',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({parent_kind:'workspace',parent_id:ws.id,title:'Approval board',kind:'analysis',start_fen:'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'})}).then(r=>r.json());
    return b.id;
  })()`);

  // Grant board writes for the app, so the policy stops asking.
  const openedPanel = await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.startsWith("Approvals"));if(!b)return false;b.click();return true;})()`);
  await sleep(1200);
  const granted = await ev(`(()=>{const label=[...document.querySelectorAll(".auto-approve label")].find(x=>x.textContent.trim()==="board-write");if(!label)return false;const box=label.querySelector('input[type="checkbox"]');box.click();return true;})()`);
  await sleep(1200);
  const stored = await ev(`(async()=>{
    const list = await fetch('/api/ai/sessions').then(r=>r.json());
    const session = list.find(s => s.permissions_json && s.permissions_json.includes('board-write'));
    return session ? session.permissions_json : null;
  })()`);

  await type("please play e4");
  await sleep(200);
  await clickText(".chat-input button", "Send");
  await waitFor(async () => await ev(`!document.querySelector(".chat-msg.streaming")`), 15000);
  await sleep(1200);

  const pending = await ev(`fetch('/api/ai/approvals').then(r=>r.json()).then(v=>v.pending.length)`);
  const board_state = await ev(`(async()=>{
    const boardId = ${JSON.stringify("%BOARD%")};
    const b = await fetch('/api/boards/' + boardId).then(r=>r.json());
    return { revision: b.revision, content: b.content_json };
  })()`.replace('%BOARD%', board));

  console.log(JSON.stringify({ board, openedPanel, granted, stored, pending, board_state }));
  const applied = board_state && board_state.revision === 2 && /"moves":\["e4"\]/.test(board_state.content || "");
  const ok = !!board && openedPanel && granted && String(stored || "").includes("board-write")
    && pending === 0 && applied;
  console.log(ok ? "AI_SCOPES_PASS" : "AI_SCOPES_FAIL");
  ws.close();
  chrome.kill();
  provider.close();
  process.exit(ok ? 0 : 1);
})().catch((error) => { console.error(error); process.exit(1); });
