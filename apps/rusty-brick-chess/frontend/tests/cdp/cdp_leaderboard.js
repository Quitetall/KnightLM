const { openView } = require("./navigation");
// The leaderboard and the tournament panel: a ladder that reflects real games,
// and a tournament you can start, pause and step from the UI.
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9545);
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-lb-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (text) => await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);

  if (!await openView(ev, "workbench", "tournaments")) throw new Error("tournaments destination unavailable");
  await sleep(1500);
  const hasBoard = await ev(`!!document.querySelector(".leaderboard")`);
  const hasTournaments = await ev(`!!document.querySelector(".tournaments")`);

  // The harness supplies independently known ratings, turns and move attempts.
  if (process.env.RBC_CDP_SEEDED_LEADERBOARD === "1") {
    const ladderRows = () => ev(`[...document.querySelectorAll(".leaderboard-table tbody tr")].map(row=>
      [...row.cells].map(cell=>cell.textContent.trim()))`);
    const filter = value => ev(`(() => {
      const input = document.querySelector(".board-filter");
      input.value = ${JSON.stringify(value)};
      input.dispatchEvent(new Event("input",{bubbles:true}));
    })()`);
    const quartz = ["llm:fixture:Quartz","1701","50","1601","8","50%","2","50"];
    const zinc = ["llm:fixture:Zinc","1550","110","1330","4","100%","0","0"];
    const assertRows = async (expected, label) => {
      for (let i = 0; i < 30; i++) {
        if (JSON.stringify(await ladderRows()) === JSON.stringify(expected)) return;
        await sleep(100);
      }
      throw new Error(label + ": " + JSON.stringify(await ladderRows()));
    };
    await assertRows([quartz,zinc], "fixture ratings or usage differ");
    await filter("qUaRtZ");
    await assertRows([quartz], "case-insensitive filter");
    await filter("unmatched model");
    await assertRows([], "empty filtered ladder");
    await filter("");
    await assertRows([quartz,zinc], "filter clear");
    console.log("Verified seeded ratings, usage, filter, and empty state");
  }

  // The create control is a form now (name, format, competitors) rather than a
  // button with four hardcoded models, so fill it before creating.
  const typeInto = async (selector, text) => await ev(`(()=>{const el=document.querySelector(${JSON.stringify(selector)});if(!el)return false;el.value=${JSON.stringify(text)};el.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  const named = await typeInto(".tournament-name", "Leaderboard cup");
  const typed = await typeInto(".tournament-competitors", "llm:a:v1, llm:b:v1, llm:c:v1, llm:d:v1");
  const created = await clickText("New round robin");
  await sleep(2000);
  const rows = await ev(`document.querySelectorAll(".tournament-list .tournament-row").length`);
  const stepClicked = await clickText("Step");
  await sleep(1500);
  const stepped = await ev(`(()=>{const e=[...document.querySelectorAll(".tournament-log li")].map(x=>x.textContent.trim()).filter(Boolean);return e.join(" | ")})()`);
  const paused = await clickText("Pause");
  await sleep(1200);
  const afterPause = await ev(`(()=>{const e=[...document.querySelectorAll(".tournament-log li")].map(x=>x.textContent.trim()).filter(Boolean);return e.join(" | ")})()`);
  const standings = await ev(`document.querySelectorAll(".standings-table tbody tr").length`);
  const columns = await ev(`[...document.querySelectorAll(".leaderboard-table thead th")].map(t=>t.textContent.trim())`);

  console.log(JSON.stringify({ hasBoard, hasTournaments, created, rows, stepClicked, stepped, paused, afterPause, standings, columns }));
  const ok =
    named && typed && hasBoard && hasTournaments && created && rows >= 1 && stepClicked
    && /started \d+ game/.test(stepped || "") && paused && /paused/.test(afterPause || "")
    && standings >= 2
    && columns.includes("Legal rate") && columns.includes("Turns") && columns.includes("Tokens");
  console.log(ok ? "AI_LEADERBOARD_PASS" : "AI_LEADERBOARD_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
