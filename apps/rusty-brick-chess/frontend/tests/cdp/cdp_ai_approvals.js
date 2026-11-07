// A queued write appears in an already-open panel and applies only on approval.
// Run against an app with MODEL_URL=http://127.0.0.1:8100/v1/chat.
const { spawn } = require("node:child_process");
const { once } = require("node:events");
const { mkdtempSync, readFileSync, rmSync } = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const BASE = "http://127.0.0.1:8090";
const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function waitFor(read, timeout = 12000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const value = await read();
    if (value) return value;
    await sleep(150);
  }
  return null;
}

async function main() {
  let boardId;
  const provider = http.createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const { messages } = JSON.parse(body);
      const followUp = messages.some((message) => message.role === "tool");
      const delta = followUp ? { content: "I requested the move." } : {
        tool_calls: [{ index: 0, id: "call_play", function: {
          name: "play_move",
          arguments: JSON.stringify({ board_id: boardId, move: "e4", expected_revision: 1 }),
        } }],
      };
      res.writeHead(200, { "content-type": "text/event-stream" });
      const send = (choice) => res.write(`data: ${JSON.stringify({ id: "mock", choices: [choice] })}\n\n`);
      send({ index: 0, delta });
      send({ index: 0, delta: {}, finish_reason: followUp ? "stop" : "tool_calls" });
      res.end("data: [DONE]\n\n");
    });
  });
  const profile = mkdtempSync(path.join(os.tmpdir(), "rbc-cdp-approvals-"));
  let chrome, ws;
  try {
    provider.listen(8100, "127.0.0.1");
    await once(provider, "listening");
    chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", "--no-first-run",
      "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--remote-allow-origins=*",
      "--window-size=1500,1000", `${BASE}/`], { stdio: ["ignore", "ignore", "pipe"] });
    let startupLog = "";
    chrome.stderr.on("data", (data) => { startupLog = (startupLog + data).slice(-1600); });
    const target = await waitFor(async () => {
      try {
        const [port] = readFileSync(path.join(profile, "DevToolsActivePort"), "utf8").split("\n");
        const response = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(1000) });
        return (await response.json()).find((tab) => tab.type === "page" && tab.url === `${BASE}/`);
      } catch { return null; }
    }, 20000);
    if (!target) throw new Error(`Chrome did not open RustyBrickChess\n${startupLog}`);
    ws = new WebSocket(target.webSocketDebuggerUrl);
    await once(ws, "open");
    let id = 0;
    const pending = new Map();
    ws.onmessage = (event) => {
      const reply = JSON.parse(event.data);
      pending.get(reply.id)?.(reply);
    };
    const evaluate = (expression) => new Promise((resolve, reject) => {
      const request = ++id;
      const timer = setTimeout(() => {
        pending.delete(request);
        reject(new Error(`CDP evaluation ${request} timed out`));
      }, 5000);
      pending.set(request, (reply) => {
        clearTimeout(timer);
        pending.delete(request);
        if (reply.error || reply.result?.exceptionDetails) {
          reject(new Error(`CDP evaluation failed: ${reply.error?.message || reply.result.exceptionDetails.text}`));
        } else {
          resolve(reply.result.result.value);
        }
      });
      ws.send(JSON.stringify({ id: request, method: "Runtime.evaluate",
        params: { expression, returnByValue: true, awaitPromise: true } }));
    });
    const click = (selector, label, prefix = false) => evaluate(`(() => {
      const button = [...document.querySelectorAll(${JSON.stringify(selector)})].find((element) =>
        ${prefix ? `element.textContent.trim().startsWith(${JSON.stringify(label)})` : `element.textContent.trim() === ${JSON.stringify(label)}`});
      button?.click();
      return !!button;
    })()`);
    if (!await waitFor(() => click(".nav", "Assistant"))) throw new Error("Assistant did not load");
    if (!await waitFor(() => click("button", "New chat"))) throw new Error("New chat button did not load");
    if (!await waitFor(() => evaluate(`!!document.querySelector(".chat-input textarea")`))) {
      throw new Error("New chat did not open");
    }

    boardId = await evaluate(`(async () => {
      async function create(route, body) {
        const response = await fetch(route, { method: "POST", headers: { "content-type": "application/json" },
          body: JSON.stringify(body) });
        if (!response.ok) throw new Error(route + " failed");
        return response.json();
      }
      const workspace = await create("/api/workspaces", { title: "Approvals", kind: "analysis" });
      const board = await create("/api/boards", { parent_kind: "workspace", parent_id: workspace.id,
        title: "Approval board", kind: "analysis", start_fen: ${JSON.stringify(START)} });
      return board.id;
    })()`);
    const readBoard = () => evaluate(`(async () => {
      const board = await fetch("/api/boards/" + ${JSON.stringify(boardId)}).then((response) => response.json());
      return { revision: board.revision, moves: JSON.parse(board.content_json).moves ?? [] };
    })()`);
    if (!await click("button", "Approvals", true)) throw new Error("Approvals button is missing");
    const empty = await waitFor(() => evaluate(`document.querySelector(".approval-queue")?.textContent.includes("Nothing is waiting for you")`));
    await evaluate(`(() => {
      const input = document.querySelector(".chat-input textarea");
      input.value = "Please play e4";
      input.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    if (!await click(".chat-input button", "Send")) throw new Error("Send button is missing");

    const queued = await waitFor(() => evaluate(`(async () => {
      const response = await fetch("/api/ai/approvals");
      return response.ok && (await response.json()).pending.some((approval) =>
        approval.tool === "play_move" && approval.target_id === ${JSON.stringify(boardId)});
    })()`), 12000);
    const before = await readBoard();
    const visible = await waitFor(() => evaluate(`document.querySelector(".approval-row .tool-chip")?.textContent.trim() === "play_move"`), 5000);
    const approved = visible && await click(".approval-row button", "Approve");
    const applied = approved && await waitFor(() => evaluate(`(() => {
      const queue = document.querySelector(".approval-queue");
      return queue && !queue.querySelector(".approval-row") && queue.textContent.includes("applied · played e4");
    })()`));
    const after = await readBoard();
    console.log(JSON.stringify({ empty, queued, before, visible, approved, applied, after }));
    return empty && queued && before.revision === 1 && before.moves.length === 0 && visible && approved && applied
      && after.revision === 2 && JSON.stringify(after.moves) === '["e4"]';
  } finally {
    ws?.close();
    provider.closeAllConnections();
    provider.close();
    if (chrome?.kill() && chrome.exitCode === null && chrome.signalCode === null) {
      await Promise.race([once(chrome, "exit"), sleep(1500)]);
    }
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
}

main().then((ok) => {
  console.log(ok ? "AI_APPROVALS_PASS" : "AI_APPROVALS_FAIL");
  process.exitCode = ok ? 0 : 1;
}).catch((error) => {
  console.error(error);
  console.log("AI_APPROVALS_FAIL");
  process.exitCode = 1;
});
