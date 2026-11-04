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

  // Seed one pair with a single chapter each and DIFFERENT moves, so the first
  // diff row has two real positions and the ply control has somewhere to go.
  // Titles carry a timestamp because every run adds to the same database and
  // matching "the newest rows" proved unreliable.
  const stamp = Date.now().toString().slice(-6);
  const leftTitle = "Left " + stamp;
  const rightTitle = "Right " + stamp;
  const seeded = await ev(`(async () => {
    const post = async (p, b) => (await fetch(p, { method: "POST", headers: {"content-type":"application/json"}, body: JSON.stringify(b) })).json();
    const seed = async (title, moves) => {
      const study = await post("/api/studies", {title});
      const chapter = await post("/api/studies/" + study.id + "/chapters", {title: "Openings"});
      const boardId = chapter.board.id;
      const board = await (await fetch("/api/boards/" + boardId)).json();
      await fetch("/api/boards/" + boardId, {
        method: "PUT",
        headers: {"content-type":"application/json"},
        body: JSON.stringify({ expected_revision: board.revision, content_json: JSON.stringify({ moves, arrows: [], circles: [], notes_md: "" }) }),
      });
      return title;
    };
    await seed(${JSON.stringify(leftTitle)}, ["e4","e5"]);
    await seed(${JSON.stringify(rightTitle)}, ["e4","e5","Nf3"]);
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
    const leftRow=find(${JSON.stringify(leftTitle)}), rightRow=find(${JSON.stringify(rightTitle)});
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

  // Side-by-side boards: clicking a change fills both boards and one ply moves
  // them together.
  // Click the row that actually has moves: the seeded board is the one-sided
  // chapter, and a row with two empty boards would prove nothing about ply.
  const clickedWithMoves = await ev(`(()=>{
    const rows=[...document.querySelectorAll(".diff-row")];
    const added=rows.find(r=>r.querySelector(".diff-change").textContent.trim()==="added");
    const target=added||rows[0];
    if(!target) return false;
    target.click();
    return true;
  })()`);
  await sleep(700);
  const boards = await ev(`document.querySelectorAll(".compare-boards .compare-side").length`);
  const plyLabel = await ev(`document.querySelector(".ply-label")?.textContent.trim()`);
  // Clicking a row jumps both boards to the first divergence, so "forward" may
  // already be at the end; "back" always has somewhere to go.
  const advanced = await ev(`(()=>{const b=document.querySelector(".ply-back");if(!b||b.disabled)return false;b.click();return true;})()`);
  await sleep(600);
  const plyAfter = await ev(`document.querySelector(".ply-label")?.textContent.trim()`);
  const plyTotal = await ev(`document.querySelector(".ply-label")?.textContent.match(/of (\\d+)/)?.[1]`);

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

  // A completed chapter shows a badge: mark the right study's chapter done, then
  // re-run the comparison so the overlay is in the report.
  const markedDone = await ev(`(async () => {
    const studies = await (await fetch("/api/studies")).json();
    const study = studies.find(s => s.title === ${JSON.stringify(rightTitle)}) || studies[studies.length - 1];
    const chapters = await (await fetch("/api/studies/" + study.id + "/chapters")).json();
    const r = await fetch("/api/studies/" + study.id + "/progress", {
      method: "PUT", headers: {"content-type":"application/json"},
      body: JSON.stringify({ chapter_id: chapters[0].id, board_id: chapters[0].primary_board_id, user: "you", completed: true }),
    });
    return r.ok;
  })()`);
  await ev(`document.querySelector(".compare-run").click()`);
  await sleep(1500);
  const badges = await ev(`document.querySelectorAll(".progress-badge").length`);

  // The ask button hands the comparison to the assistant: it switches tab and
  // prefills the composer with a question naming both studies.
  const asked = await ev(`(()=>{const b=document.querySelector(".compare-ask");if(!b)return false;b.click();return true;})()`);
  await sleep(1200);
  const onAssistant = await ev(`Boolean(document.querySelector(".assistant")) || document.body.innerText.includes("Assistant")`);
  const draftAfter = await ev(`(()=>{const t=document.querySelector(".assistant textarea, textarea");return t?t.value:"";})()`);

  await ev(`[...document.querySelectorAll("button")].find(b=>b.textContent.trim()==="Compare")?.click()`);
  await sleep(1000);
  await ev(`document.querySelector(".scope-clear").click()`);
  await sleep(1400);
  const afterClear = await ev(`(async () => (await (await fetch("/api/compare/scope")).json()))()`);
  const chipGone = await ev(`!document.querySelector(".scope-chip")`);

  console.log(JSON.stringify({ seeded, picked, hasView, rows, chip, diffRows, summary, changedCount, allCount, focused, clickedWithMoves, boards, plyLabel, advanced, plyAfter, plyTotal, markedDone, badges, asked, onAssistant, draftAfter, exportClicked, exported, stored, afterClear, chipGone }));
  const ok = seeded && picked && hasView && rows >= 2
    && Boolean(chip) && chip.includes(leftTitle) && chip.includes(rightTitle)
    && diffRows >= 1
    && changedCount >= 1 && allCount >= changedCount && focused === true
    && clickedWithMoves === true && boards === 2
    && Boolean(plyLabel) && plyAfter !== plyLabel
    && markedDone === true && badges >= 1
    && asked === true && draftAfter.includes(leftTitle)
    && exported === "diff.md"
    && stored.active === true
    && afterClear.active === false && chipGone === true;
  console.log(ok ? "AI_COMPARE_PASS" : "AI_COMPARE_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
