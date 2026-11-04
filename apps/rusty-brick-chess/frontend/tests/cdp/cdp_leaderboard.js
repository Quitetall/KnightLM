// The leaderboard and the tournament panel: a ladder that reflects real games,
// and a tournament you can start, pause and step from the UI.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9545;
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-lb-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (text) => await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);

  // Seed a rated game so the ladder has a row to show.
  const seeded = await ev(`(async()=>{
    const post = (p, b) => fetch(p, {method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify(b||{})}).then(r=>r.json());
    const white = 'llm:a:v1', black = 'llm:b:v1';
    const period = Date.now();
    await post('/api/debug/stage-rating', {period, white, black});
    return true;
  })()`).catch(() => false);

  await clickText("Play");
  await sleep(1500);
  const hasBoard = await ev(`!!document.querySelector(".leaderboard")`);
  const hasTournaments = await ev(`!!document.querySelector(".tournaments")`);

  const created = await clickText("New round robin");
  await sleep(2000);
  const rows = await ev(`document.querySelectorAll(".tournament-list .tournament-row").length`);
  const stepClicked = await clickText("Step");
  await sleep(1500);
  const stepped = await ev(`(()=>{const e=[...document.querySelectorAll(".match-panel .muted")].map(x=>x.textContent.trim()).filter(Boolean);return e.join(" | ")})()`);
  const paused = await clickText("Pause");
  await sleep(1200);
  const afterPause = await ev(`(()=>{const e=[...document.querySelectorAll(".match-panel .muted")].map(x=>x.textContent.trim()).filter(Boolean);return e.join(" | ")})()`);
  const standings = await ev(`document.querySelectorAll(".standings-table tbody tr").length`);

  console.log(JSON.stringify({ hasBoard, hasTournaments, created, rows, stepClicked, stepped, paused, afterPause, standings }));
  const ok = hasBoard && hasTournaments && created && rows >= 1 && stepClicked
    && /started \d+ game/.test(stepped || "") && paused && /paused/.test(afterPause || "")
    && standings >= 2;
  console.log(ok ? "AI_LEADERBOARD_PASS" : "AI_LEADERBOARD_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
