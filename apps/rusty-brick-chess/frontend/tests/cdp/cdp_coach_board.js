// A coach turn through the real app: a scripted OpenAI-compatible model answers
// with a legal move, two marks and a pointer at an earlier ply, and the board
// draws them. Self-contained: it starts the model fake, the server and Chrome.
//
//   RBC_BIN=../../../target/debug/rusty-brick-chess node cdp_coach_board.js
//   (needs the frontend deployed: ../../deploy.sh)
const { spawn } = require("child_process");
const http = require("http");
const os = require("os");
const path = require("path");
const fs = require("fs");
const CHROME_PORT = 9561, APP_PORT = 8191, MODEL_PORT = 8199;
const BIN = process.env.RBC_BIN || path.join(__dirname, "../../../target/debug/rusty-brick-chess");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function getJson(port, p) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path: p }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const COACH = [
  "Provisional: e7e5",
  '<tool_call>{"name":"mark_arrow","arguments":{"from":"e7","to":"e5","color":"red"}}</tool_call>',
  '<tool_call>{"name":"mark_square","arguments":{"square":"e4","color":"blue"}}</tool_call>',
  '<tool_call>{"name":"show_game_move","arguments":{"ply":1}}</tool_call>',
  "I mirror the centre pawn.",
].join("\n");
const model = http.createServer((req, res) => {
  let body = ""; req.on("data", (c) => (body += c));
  req.on("end", () => {
    if (req.url.endsWith("/models")) { res.setHeader("content-type", "application/json"); return res.end(JSON.stringify({ data: [{ id: "fake-model" }] })); }
    const wantsStream = JSON.parse(body || "{}").stream === true;
    if (wantsStream) {
      res.setHeader("content-type", "text/event-stream");
      const chunk = (delta, finish) => `data: ${JSON.stringify({ id: "c", object: "chat.completion.chunk", model: "fake-model", choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`;
      return res.end(chunk({ role: "assistant", content: COACH }, null) + chunk({}, "stop") + "data: [DONE]\n\n");
    }
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify({ id: "c", object: "chat.completion", model: "fake-model", choices: [{ index: 0, message: { role: "assistant", content: COACH }, finish_reason: "stop" }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }));
  });
}).listen(MODEL_PORT, "127.0.0.1");
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "rbc-coach-"));
const app = spawn(BIN, [], { env: { ...process.env, RBC_DB: path.join(tmp, "rbc.db"), RBC_ADDR: `127.0.0.1:${APP_PORT}`, RBC_STATIC: path.join(__dirname, "../../public") }, stdio: "ignore" });
let chrome;
const cleanup = () => { try { chrome && chrome.kill(); } catch {} try { app.kill(); } catch {} model.close(); fs.rmSync(tmp, { recursive: true, force: true }); };
(async () => {
  for (let i = 0; i < 50; i++) { await sleep(200); try { await getJson(APP_PORT, "/api/health"); break; } catch {} }
  chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${CHROME_PORT}`, `--user-data-dir=${path.join(tmp, "chrome")}`, "--remote-allow-origins=*", "--window-size=1500,1000", `http://127.0.0.1:${APP_PORT}/`], { stdio: "ignore" });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(CHROME_PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2500);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (sel, text) => await ev(`(()=>{const b=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);

  // A registered provider for the fake model, and a game waiting for the model's reply.
  await ev(`(async()=>{
    const post = (p, b) => fetch(p, {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(b||{})}).then(r=>r.json());
    const prov = await post('/api/providers', {label:'Fake', base_url:'http://127.0.0.1:${MODEL_PORT}/v1'});
    const m = await post('/api/ai/matches', {config:{start_fen:'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
      white:{kind:'human',name:'You'}, black:{kind:'model',provider:'provider:'+prov.id,model:'fake-model'}, max_plies:300, rated:false}});
    await post('/api/ai/matches/' + m.id + '/move', {mv:'e2e4'});
    return {id: m.id};
  })()`);
  await ev("location.reload()"); await sleep(2600);
  await clickText(".nav", "Play"); await sleep(1200);
  const chatToggle = await ev(`!!document.querySelector(".play-chat-toggle") && document.querySelector(".play-chat-toggle").checked`);
  await ev(`(()=>{const c=document.querySelector(".match-card");if(c)c.click();})()`); await sleep(1500);
  await ev(`(()=>{const b=document.querySelector(".match-model-move");if(b)b.click();})()`); await sleep(2500);

  const arrows = await ev(`document.querySelectorAll("svg.shapes line").length`);
  const pointed = await ev(`[...document.querySelectorAll(".match-moves .ply.pointed")].map(p=>p.textContent.trim())`);
  const pointerBox = await ev(`!!document.querySelector(".coach-pointer")`);
  const chatLines = await ev(`[...document.querySelectorAll(".chat-line.assistant")].map(p=>p.textContent.trim())`);
  const moves = await ev(`[...document.querySelectorAll(".match-moves .ply")].map(p=>p.textContent.trim())`);

  // Chat off: the panel gives way to its note, and the choice is stored.
  await ev(`(()=>{const c=document.querySelector(".play-chat-toggle");c.click();})()`); await sleep(800);
  const offNote = await ev(`!document.querySelector(".chat-log") && !!document.querySelector(".match-chat .muted")`);
  const stored = await ev(`fetch('/api/model-seats').then(r=>r.json())`);

  const out = { chatToggle, arrows, pointed, pointerBox, chatLines, moves, offNote, stored };
  console.log(JSON.stringify(out));
  const ok = chatToggle === true && arrows >= 1 && pointed.length === 1 && /e4/.test(pointed[0])
    && pointerBox === true && chatLines.length === 1 && /mirror the centre/.test(chatLines[0])
    && moves.length === 2 && offNote === true && stored.chat === false;
  console.log(ok ? "COACH_BOARD_PASS" : "COACH_BOARD_FAIL");
  ws.close(); cleanup(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); cleanup(); process.exit(1); });
