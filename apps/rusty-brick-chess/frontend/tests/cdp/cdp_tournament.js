// A tournament from the UI: create with a format and competitors, step it, read
// the pairings and standings, pause, and export the PGN.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9546;
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-tour-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (text) => await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);
  const typeInto = async (selector, text) => await ev(`(()=>{const el=document.querySelector(${JSON.stringify(selector)});if(!el)return false;el.value=${JSON.stringify(text)};el.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);

  await clickText("Play");
  await sleep(1500);
  const named = await typeInto(".tournament-name", "UI cup");
  const typed = await typeInto(".tournament-competitors", "llm:a:v1, llm:b:v1, llm:c:v1, llm:d:v1");
  const created = await clickText("New round robin");
  await sleep(2200);
  const rows = await ev(`document.querySelectorAll(".tournament-list .tournament-row").length`);

  const stepped = await clickText("Step");
  await sleep(1600);
  const pairings = await ev(`[...document.querySelectorAll(".pairings .pairing")].map(p=>p.textContent.trim())`);
  const standings = await ev(`document.querySelectorAll(".standings-table tbody tr").length`);
  const log = await ev(`[...document.querySelectorAll(".tournament-log li")].map(l=>l.textContent.trim())`);

  const paused = await clickText("Pause");
  await sleep(1200);
  const pausedLog = await ev(`[...document.querySelectorAll(".tournament-log li")].map(l=>l.textContent.trim()).join(" | ")`);

  // Export must offer a file.
  // Keep the spy installed while the export fetches: it is async, and restoring
  // the prototype immediately would miss the click it eventually makes.
  const exportClicked = await ev(`(()=>{
    window.__downloaded = null;
    window.__restoreClick = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function () { window.__downloaded = this.download; };
    const b = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === "Export PGN");
    if (!b) { HTMLAnchorElement.prototype.click = window.__restoreClick; return "no button"; }
    b.click();
    return "clicked";
  })()`);
  await sleep(1800);
  const exported = await ev(`(()=>{
    HTMLAnchorElement.prototype.click = window.__restoreClick;
    return window.__downloaded;
  })()`);

  console.log(JSON.stringify({ named, typed, created, rows, stepped, pairings, standings, log, paused, pausedLog, exportClicked, exported }));
  const ok = named && typed && created && rows >= 1 && stepped
    && pairings.length >= 2 && standings >= 4
    && log.join(" ").includes("created UI cup")
    && paused && /paused/.test(pausedLog || "")
    && exported === "match.pgn";
  console.log(ok ? "AI_TOURNAMENT_PASS" : "AI_TOURNAMENT_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
