// Watching two models: step, pause, resume, reset, and the annotated PGN.
// There is no watch UI yet (that is t-f26f), so this drives the routes from the
// page's own origin, which is what the UI will call.
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9542);
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const DRIVER = `(async () => {
  const post = (path, body) => fetch(path, {
    method: 'POST',
    headers: {'content-type': 'application/json'},
    body: JSON.stringify(body || {}),
  }).then((r) => r.json());
  const seats = {
    white: {kind: 'model', provider: 'mock', model: 'w'},
    black: {kind: 'model', provider: 'mock', model: 'b'},
  };
  const m = await post('/api/ai/matches', {config: {
    start_fen: 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
    ...seats, max_plies: 300, rated: false,
    watch: {white_reasoning: true, black_reasoning: false},
  }});
  const id = m.id;
  if (!id) return {error: 'no match', payload: m};
  const stepped = await post('/api/ai/matches/' + id + '/watch/step', {plies: 2});
  const pause = await post('/api/ai/matches/' + id + '/watch/pause');
  const whilePaused = await post('/api/ai/matches/' + id + '/watch/step', {plies: 1});
  const resume = await post('/api/ai/matches/' + id + '/watch/resume');
  const reset = await post('/api/ai/matches/' + id + '/watch/reset');
  return {
    id,
    plies: (stepped.steps || []).map((s) => [s.mv.san, s.annotated]),
    pgnLine: (stepped.match && stepped.match.pgn || '').split('\\n').filter((l) => /^1\\./.test(l)).join(''),
    paused: pause.paused,
    pausedRefusal: whilePaused.error,
    resumedState: resume.state,
    resumedPaused: resume.paused,
    afterReset: reset.match ? reset.match.moves.length : -1,
  };
})().then((r) => JSON.stringify(r), (e) => JSON.stringify({error: String(e)}))`;

(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-watch-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1400,900", (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2500);
  const raw = (await send("Runtime.evaluate", { expression: DRIVER, returnByValue: true, awaitPromise: true })).result?.value;
  const out = JSON.parse(raw || "{}");
  console.log(raw);
  const annotated = (out.plies || []).map((p) => p[1]);
  const ok = !out.error && out.plies.length === 2
    && annotated[0] === true && annotated[1] === false
    && /controls the centre/.test(out.pgnLine || "")
    && out.paused === true && /paused/.test(out.pausedRefusal || "")
    && out.resumedPaused === false && out.resumedState === "awaiting_white"
    && out.afterReset === 0;
  console.log(ok ? "AI_WATCH_PASS" : "AI_WATCH_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
