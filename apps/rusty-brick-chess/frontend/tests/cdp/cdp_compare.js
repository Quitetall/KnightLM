// Compare: pick two studies, see the scope chip, read the diff rows, clear it.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9552;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-compare-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;

  // Seed two studies with a shared chapter through the API.
  const seeded = await ev(`(async () => {
    const post = async (p, b) => (await fetch(p, { method: "POST", headers: {"content-type":"application/json"}, body: JSON.stringify(b) })).json();
    const a = await post("/api/studies", {title: "Left study"});
    const b = await post("/api/studies", {title: "Right study"});
    await post("/api/studies/" + a.id + "/chapters", {title: "Openings"});
    // A chapter both sides share and that nothing changes, so the "only changes"
    // filter has something to hide.
    await post("/api/studies/" + a.id + "/chapters", {title: "Same"});
    await post("/api/studies/" + b.id + "/chapters", {title: "Same"});
    const chapter = await post("/api/studies/" + b.id + "/chapters", {title: "Openings"});
    const boardId = chapter.board.id;
    const board = await (await fetch("/api/boards/" + boardId)).json();
    await fetch("/api/boards/" + boardId, {
      method: "PUT",
      headers: {"content-type":"application/json"},
      body: JSON.stringify({ expected_revision: board.revision, content_json: JSON.stringify({ moves: ["e4","e5","Nf3"], arrows: [], circles: [], notes_md: "" }) }),
    });
    return true;
  })()`);

  await ev(`[...document.querySelectorAll("button")].find(b=>b.textContent.trim()==="Compare")?.click()`);
  await sleep(1400);
  const hasView = await ev(`Boolean(document.querySelector(".compare-view"))`);
  const rows = await ev(`document.querySelectorAll(".compare-row").length`);
  // Pick A and B on the two newest rows, then run.
  // Select by title: the list holds studies from earlier runs too, so row order
  // is not a stable way to pick the pair this test just created.
  const picked = await ev(`(()=>{
    const rows=[...document.querySelectorAll(".compare-row")];
    const find=(title)=>rows.find(r=>r.querySelector(".compare-title").textContent.trim()===title);
    const leftRow=find("Left study"), rightRow=find("Right study");
    if(!leftRow||!rightRow) return false;
    leftRow.querySelector(".pick-left").click();
    rightRow.querySelector(".pick-right").click();
    return true;
  })()`);
  await ev(`document.querySelector(".compare-run").click()`);
  await sleep(1800);
  const chip = await ev(`document.querySelector(".scope-chip")?.textContent.trim()`);
  const diffRows = await ev(`document.querySelectorAll(".diff-row").length`);
  const summary = await ev(`document.querySelector(".compare-summary")?.textContent.trim()`);
  const stored = await ev(`(async () => (await (await fetch("/api/compare/scope")).json()))()`);

  // Filters and focus: the changed filter keeps a changed row, the all filter
  // keeps an unchanged one too, and clicking a row focuses it.
  const changedCount = await ev(`document.querySelectorAll(".diff-row").length`);
  await ev(`[...document.querySelectorAll(".filter-button")].find(b=>b.textContent.trim()==="Everything")?.click()`);
  await sleep(600);
  const allCount = await ev(`document.querySelectorAll(".diff-row").length`);
  await ev(`document.querySelector(".diff-row")?.click()`);
  await sleep(400);
  const focused = await ev(`Boolean(document.querySelector(".diff-row.focused"))`);

  // Export offers a file.
  const exportClicked = await ev(`(()=>{
    window.__downloaded = null;
    window.__restore = HTMLAnchorElement.prototype.click;
    HTMLAnchorElement.prototype.click = function(){ window.__downloaded = this.download; };
    const b = document.querySelector(".compare-export");
    if (!b) return "no button";
    b.click();
    return "clicked";
  })()`);
  await sleep(1600);
  const exported = await ev(`(()=>{ HTMLAnchorElement.prototype.click = window.__restore; return window.__downloaded; })()`);

  await ev(`document.querySelector(".scope-clear").click()`);
  await sleep(1400);
  const afterClear = await ev(`(async () => (await (await fetch("/api/compare/scope")).json()))()`);
  const chipGone = await ev(`!document.querySelector(".scope-chip")`);

  console.log(JSON.stringify({ seeded, picked, hasView, rows, chip, diffRows, summary, changedCount, allCount, focused, exportClicked, exported, stored, afterClear, chipGone }));
  const ok = seeded && picked && hasView && rows >= 2
    && Boolean(chip) && chip.includes("Left study") && chip.includes("Right study")
    && diffRows >= 1
    && changedCount >= 1 && allCount >= changedCount && focused === true
    && exported === "diff.md"
    && stored.active === true
    && afterClear.active === false && chipGone === true;
  console.log(ok ? "AI_COMPARE_PASS" : "AI_COMPARE_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
