// The Play tab: cards, clocks, moves, controls, and the three handoffs.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9543;
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-playui-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (sel, text) => await ev(`(()=>{const b=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);

  const onPlay = await clickText(".nav", "Play");
  await sleep(1200);
  const hasHead = await ev(`!!document.querySelector(".play-head")`);

  // Start a model-vs-model game, then step it twice through the UI.
  const started = await clickText("button", "Watch two models");
  await sleep(2000);
  const cards = await ev(`document.querySelectorAll(".player-card").length`);
  const stepped = await clickText("button", "Step");
  await sleep(1500);
  await clickText("button", "Step");
  await sleep(1500);
  const plies = await ev(`[...document.querySelectorAll(".match-moves .ply")].map(p=>p.textContent.trim())`);
  const pgn = await ev(`(()=>{const e=document.querySelector(".match-pgn");return e?e.textContent.trim():"";})()`);
  const listCards = await ev(`document.querySelectorAll(".match-card").length`);

  // A match that ends reports its result and termination, and the Step button
  // stops offering more plies.
  const shortMatch = await ev(`(async()=>{
    const post = (p, b) => fetch(p, {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(b||{})}).then(r=>r.json());
    const m = await post('/api/ai/matches', {config:{start_fen:'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
      white:{kind:'model',provider:'mock',model:'w'}, black:{kind:'model',provider:'mock',model:'b'},
      max_plies:2, rated:false, watch:{white_reasoning:false, black_reasoning:false}}});
    const stepped = await post('/api/ai/matches/' + m.id + '/watch/step', {plies: 2});
    return { id: m.id, result: stepped.match ? stepped.match.result : null };
  })()`);
  await clickText("button", "Refresh");
  await sleep(1200);
  const shortCard = await ev(`(()=>{const cards=[...document.querySelectorAll(".match-card")];const c=cards[0];if(!c)return false;c.click();return true;})()`);
  await sleep(1400);
  const resultLine = await ev(`(()=>{const e=document.querySelector(".match-result");return e?e.textContent.trim():"";})()`);
  const stepDisabled = await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()==="Step");return b?b.disabled:null;})()`);

  // Rematch makes a new game rather than reopening the old one.
  const beforeRematch = await ev(`document.querySelectorAll(".match-card").length`);
  const rematched = await clickText("button", "Rematch");
  await sleep(2000);
  const afterRematch = await ev(`document.querySelectorAll(".match-card").length`);
  const freshPlies = await ev(`document.querySelectorAll(".match-moves .ply").length`);

  // Pause, then try to step: the button must stop adding plies.
  const paused = await clickText("button", "Pause");
  await sleep(1200);
  const afterPause = await ev(`document.querySelector(".match-state")?.textContent.trim()`);

  // Save to database, which is how a match leaves this screen.
  const saved = await clickText("button", "Save to database");
  await sleep(1800);
  const message = await ev(`(()=>{const e=[...document.querySelectorAll(".match-panel .muted")].pop();return e?e.textContent.trim():"";})()`);
  const gamesInDb = await ev(`fetch('/api/games?limit=5').then(r=>r.json()).then(v=>Array.isArray(v)?v.length:(v.games||[]).length)`);

  // Export must actually offer a file, and Open in Analysis must land on the board.
  // Keep the spy installed: the export fetches asynchronously and only then
  // clicks the anchor, so restoring the prototype immediately misses it - the
  // same trap cdp_tournament hit.
  const exportClicked = await ev(`(()=>{
    window.__downloaded = null;
    window.__restoreClick = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function () { window.__downloaded = this.download; };
    const b = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === "Export PGN");
    if (!b) { HTMLAnchorElement.prototype.click = window.__restoreClick; return "no button"; }
    b.click();
    return "clicked";
  })()`);
  await sleep(1500);
  const exported = await ev(`(()=>{
    HTMLAnchorElement.prototype.click = window.__restoreClick;
    return window.__downloaded;
  })()`);

  await clickText("button", "Open in Analysis");
  await sleep(2500);
  const navAfterOpen = await ev(`(()=>{const n=document.querySelector(".nav.active");return n?n.textContent.trim():"";})()`);
  const boardLoaded = await ev(`(()=>!!document.querySelector(".board"))()`);

  console.log(JSON.stringify({ exportClicked, exported, navAfterOpen, boardLoaded, onPlay, hasHead, started, cards, plies, pgn: (pgn||"").slice(0,60), listCards, shortMatch, shortCard, resultLine, stepDisabled, beforeRematch, rematched, afterRematch, freshPlies, paused, afterPause, saved, message, gamesInDb }));
  const ok = onPlay && hasHead && started && cards === 2 && plies.length >= 2
    && /1\./.test(pgn || "") && listCards >= 1
    && shortCard && /·/.test(resultLine || "") && stepDisabled === true
    && rematched && afterRematch === beforeRematch + 1 && freshPlies === 0
    && paused && /paused/.test(afterPause || "")
    && saved && /saved/.test(message || "") && gamesInDb >= 1
    && exported === "match.pgn" && navAfterOpen === "Analysis" && boardLoaded;
  console.log(ok ? "AI_MATCH_UI_PASS" : "AI_MATCH_UI_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
