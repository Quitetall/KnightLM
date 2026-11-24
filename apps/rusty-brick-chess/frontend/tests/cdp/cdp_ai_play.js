// Human play through the app: a person moves, the model answers, and a
// competitive match refuses the social actions.
//
// There is no match UI yet (that is t-f26f), so this drives the routes from the
// page's own origin, which is what the UI will call.
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9537);
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-play-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2500);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;

  const out = await ev(`(async()=>{
    const post = (path, body) => fetch(path, {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(body||{})}).then(r=>r.json());
    const base = {start_fen:'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1', white:{kind:'human',name:'Alex'}, black:{kind:'model',provider:'mock',model:'m1'}, max_plies:300};
    const casual = await post('/api/ai/matches', {config: {...base, rated: false}});
    const rated = await post('/api/ai/matches', {config: {...base, rated: true}});

    const human = await post('/api/ai/matches/' + casual.id + '/move', {mv: 'e4'});
    const model = await post('/api/ai/matches/' + casual.id + '/model-move');
    const chat = await post('/api/ai/matches/' + casual.id + '/chat', {message: 'any advice?'});
    const back = await post('/api/ai/matches/' + casual.id + '/takeback');

    await post('/api/ai/matches/' + rated.id + '/move', {mv: 'd4'});
    const ratedChat = await post('/api/ai/matches/' + rated.id + '/chat', {message: 'hint?'});
    const ratedBack = await post('/api/ai/matches/' + rated.id + '/takeback');
    const ratedModel = await post('/api/ai/matches/' + rated.id + '/model-move');

    return {
      humanSan: human.moves?.[human.moves.length-1]?.san,
      modelSan: model.move?.san,
      modelAttempts: model.attempts,
      chatReply: (chat.reply||'').slice(0,30),
      takenBack: back.taken_back?.san,
      pliesAfterTakeback: back.match?.moves?.length,
      ratedChatError: ratedChat.error,
      ratedBackError: ratedBack.error,
      ratedModelSan: ratedModel.move?.san,
      ratedReasoning: ratedModel.reasoning,
    };
  })()`);

  console.log(JSON.stringify(out));
  const ok = out && out.humanSan === "e4" && out.modelSan === "e5" && out.modelAttempts === 1
    && /Nf3/.test(out.chatReply || "") && out.takenBack === "e5" && out.pliesAfterTakeback === 1
    && /competitive/.test(out.ratedChatError || "") && /competitive/.test(out.ratedBackError || "")
    && out.ratedModelSan === "e5" && out.ratedReasoning === null;
  console.log(ok ? "AI_PLAY_PASS" : "AI_PLAY_FAIL");
  ws.close(); chrome.kill();
  process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
