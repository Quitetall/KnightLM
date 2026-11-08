// The Play tab: cards, clocks, moves, controls, and the three handoffs.
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9543);
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-playui-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true, awaitPromise: true })).result?.value;
  const clickText = async (sel, text) => await ev(`(()=>{const b=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);

  const onPlay = await clickText(".nav", "Play");
  await sleep(1200);
  const hasHead = await ev(`!!document.querySelector(".play-head")`);

  // Start a model-vs-model game, then step it twice through the UI.
  const fixtureSelected = await ev(`(() => {
    const opponent = document.querySelector(".play-opponent");
    if (!opponent?.querySelector('option[value="0"]')) return false;
    opponent.value = "0";
    opponent.dispatchEvent(new Event("change", { bubbles: true }));
    const more = document.querySelector(".play-more");
    if (more) more.open = true;
    return true;
  })()`);
  const started = await clickText("button", "Watch two models");
  await sleep(2000);
  const cards = await ev(`document.querySelector(".match-cards strong")?.textContent.includes(" vs ")`);
  const stepped = await ev(`(()=>{const b=document.querySelector(".match-step");if(!b||b.disabled)return false;b.click();return true;})()`);
  await sleep(1500);
  await ev(`(()=>{const b=document.querySelector(".match-step");if(!b||b.disabled)return false;b.click();return true;})()`);
  await sleep(1500);
  const plies = await ev(`[...document.querySelectorAll(".match-moves .ply")].map(p=>p.textContent.trim())`);
  await ev(`document.querySelector(".match-handoff details summary")?.click()`);
  const pgn = await ev(`document.querySelector(".match-handoff details[open] .match-pgn")?.textContent.trim()`);
  const listCards = await ev(`document.querySelectorAll(".match-card").length`);

  // A match that ends reports its result and termination, and the Step button
  // stops offering more plies.
  const shortMatch = await ev(`(async()=>{
    const post = (p, b) => fetch(p, {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(b||{})}).then(r=>r.json());
    const m = await post('/api/ai/matches', {config:{start_fen:'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1',
      white:{kind:'model',provider:'mock',model:'w'}, black:{kind:'model',provider:'mock',model:'b'},
      max_plies:2, rated:false, watch:{white_reasoning:false, black_reasoning:false}}});
    const first = await post('/api/ai/matches/' + m.id + '/watch/step', {plies: 1});
    const stepped = await post('/api/ai/matches/' + m.id + '/watch/step', {plies: 1});
    return { id: m.id, fen: first.match?.fen, latestFen: stepped.match?.fen, result: stepped.match?.result };
  })()`);
  await clickText("button", "Refresh");
  await sleep(1200);
  const shortCard = await ev(`(()=>{const cards=[...document.querySelectorAll(".match-card")];const c=cards[0];if(!c)return false;c.click();return true;})()`);
  await sleep(1400);
  const resultLine = await ev(`(()=>{const e=document.querySelector(".match-status");return e?e.textContent.trim():"";})()`);
  const stepDisabled = await ev(`(()=>{const b=document.querySelector(".match-step");return b?b.disabled:null;})()`);

  // Rematch makes a new game rather than reopening the old one.
  const beforeRematch = await ev(`document.querySelectorAll(".match-card").length`);
  const rematched = await clickText("button", "Rematch");
  await sleep(2000);
  const afterRematch = await ev(`document.querySelectorAll(".match-card").length`);
  const freshPlies = await ev(`document.querySelectorAll(".match-moves .ply").length`);

  // Pause, then try to step: the button must stop adding plies.
  const paused = await ev(`(()=>{const b=document.querySelector(".match-pause");if(!b||b.disabled)return false;b.click();return true;})()`);
  await sleep(1200);
  const afterPause = await ev(`document.querySelector(".match-status")?.textContent.trim()`);

  // Save to database, which is how a match leaves this screen.
  const saved = await clickText("button", "Save to database");
  await sleep(1800);
  const message = await ev(`(()=>{const e=document.querySelector(".match-error");return e?e.textContent.trim():"";})()`);
  const gamesInDb = await ev(`fetch('/api/games?limit=5').then(r=>r.json()).then(v=>Array.isArray(v)?v.length:(v.games||[]).length)`);

  // Export must actually offer a file, and Open in Analysis must land on the board.
  // Keep the spy installed: the export fetches asynchronously and only then
  // clicks the anchor, so restoring the prototype immediately misses it - the
  // same trap cdp_tournament hit.
  const exportClicked = await ev(`(()=>{
    window.__downloaded = null;
    window.__restoreClick = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function () { window.__downloaded = this.download; };
    const b = document.querySelector(".match-export") || [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === "Export PGN");
    if (!b) { HTMLAnchorElement.prototype.click = window.__restoreClick; return "no button"; }
    b.click();
    return "clicked";
  })()`);
  await sleep(1500);
  const exported = await ev(`(()=>{
    HTMLAnchorElement.prototype.click = window.__restoreClick;
    return window.__downloaded;
  })()`);

  await ev(`document.querySelector('[data-match-id="${shortMatch.id}"]')?.click()`);
  await sleep(1400);
  const replayed = await ev(`(() => {
    const first = document.querySelector('[data-match-ply="1"]');
    if (!first) return false;
    first.click();
    return true;
  })()`);
  await sleep(200);
  const activePly = await ev(`document.querySelector('[data-match-ply][aria-pressed="true"]')?.dataset.matchPly`);
  const reviewing = await ev(`document.querySelector(".active-match [role=status]:not(.match-status)")?.textContent`);
  await ev(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (request, init) => {
      const response = await original(request, init);
      const url = typeof request === "string" ? request : request.url;
      if (new URL(url, location.href).pathname === "/api/games" && (init?.method || request.method) === "POST") {
        window.__importedMatch = (await response.clone().json()).imported?.[0];
        const other = await original("/api/games", {
          method:"POST", headers:{"Content-Type":"application/json"},
          body:JSON.stringify({pgn:'[White "Decoy"]\\n[Black "Other import"]\\n[Result "*"]\\n\\n1. d4 *'}),
        });
        window.__otherGame = (await other.json()).imported?.[0];
      }
      return response;
    };
  })()`);
  await clickText("button", "Open in Analysis");
  await sleep(3000);
  const navAfterOpen = await ev(`(()=>{return document.querySelector("main")?.dataset.view;})()`);
  const boardLoaded = await ev(`(()=>!!document.querySelector(".board"))()`);
  const handoff = await ev(`({
    imported:window.__importedMatch, other:window.__otherGame,
    fen:document.querySelector(".fen-line code")?.textContent,
    title:document.querySelector(".ws-tab.active .ws-name")?.textContent,
  })`);
  const modes = await ev(`(() => {
    const analysis = document.querySelector(".analysis");
    const before = analysis?.dataset.analysisMode;
    analysis?.querySelector('[data-analysis-view="detailed"]')?.click();
    return before;
  })()`);
  await sleep(100);
  const detailed = await ev(`(() => {
    const analysis = document.querySelector(".analysis");
    const visible = !!analysis?.querySelector(".detailed-analysis:not([hidden]) .ann-editor");
    const fen = analysis?.querySelector(".fen-line code")?.textContent;
    analysis?.querySelector('[data-analysis-view="simple"]')?.click();
    return {visible, fen};
  })()`);
  await sleep(100);
  const backToSimple = await ev(`document.querySelector(".analysis")?.dataset.analysisMode === "simple"
    && document.querySelector(".fen-line code")?.textContent === ${JSON.stringify(shortMatch.fen)}`);
  await clickText(".nav", "Play");
  await sleep(1500);
  const resumed = await ev(`document.querySelector(".match-card.active")?.dataset.matchId`);
  await clickText("button", "Open in Analysis");
  await sleep(2200);
  const latestFen = await ev(`document.querySelector(".fen-line code")?.textContent`);

  await clickText(".nav", "Play");
  await sleep(500);
  const friendStarted = await clickText("button", "Play a friend");
  await sleep(1400);
  const friendId = await ev(`document.querySelector(".match-card.active")?.dataset.matchId`);
  const moveSquare = async (square) => {
    const location = await ev(`(() => {
      const bounds = document.querySelector('.active-match [data-square="${square}"]')?.getBoundingClientRect();
      return bounds && {x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2};
    })()`);
    if (!location) return false;
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", { type, ...location, button:"left", clickCount:1 });
    }
    return true;
  };
  await moveSquare("e2");
  await moveSquare("e4");
  await sleep(600);
  await moveSquare("e7");
  await moveSquare("e5");
  await sleep(800);
  const friendGame = await ev(`fetch("/api/ai/matches/${friendId}").then(response => response.json())`);
  console.log(JSON.stringify({ friendStarted, friendMoves: friendGame?.moves?.map(move => move.san), replayed, activePly, reviewing, handoff, modes, detailed, backToSimple, resumed, latestFen, exportClicked, exported, navAfterOpen, boardLoaded, onPlay, hasHead, started, cards, plies, pgn: (pgn||"").slice(0,60), listCards, shortMatch, shortCard, resultLine, stepDisabled, beforeRematch, rematched, afterRematch, freshPlies, paused, afterPause, saved, message, gamesInDb }));
  const ok = onPlay && hasHead && fixtureSelected && started && cards === true && plies.length >= 2
    && /1\./.test(pgn || "") && listCards >= 1
    && shortCard && /·/.test(resultLine || "") && stepDisabled === true
    && rematched && afterRematch === beforeRematch + 1 && freshPlies === 0
    && paused && /paused/.test(afterPause || "")
    && saved && /saved/.test(message || "") && gamesInDb >= 1
    && exported === "match.pgn" && navAfterOpen === "analysis" && boardLoaded
    && replayed && activePly === "1" && /ply 1/.test(reviewing)
    && handoff.imported && handoff.other && handoff.imported !== handoff.other
    && handoff.fen === shortMatch.fen && !/Decoy/.test(handoff.title || "")
    && resumed === shortMatch.id && latestFen === shortMatch.latestFen
    && modes === "simple" && detailed.visible && detailed.fen === shortMatch.fen && backToSimple
    && friendStarted && friendGame?.white?.kind === "human" && friendGame?.black?.kind === "human"
    && friendGame?.moves?.map(move => move.san).join(" ") === "e4 e5";
  console.log(ok ? "AI_MATCH_UI_PASS" : "AI_MATCH_UI_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
