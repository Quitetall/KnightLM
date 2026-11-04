// The Study tab: create a study, add chapters, reorder and duplicate one, add a
// board and mark it primary, write notes, and publish.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9548;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-study-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1600,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const click = async (sel) => await ev(`(()=>{const b=document.querySelector(${JSON.stringify(sel)});if(!b)return false;b.click();return true;})()`);
  const typeInto = async (sel, text) => await ev(`(()=>{const el=document.querySelector(${JSON.stringify(sel)});if(!el)return false;el.value=${JSON.stringify(text)};el.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);

  await ev(`[...document.querySelectorAll("nav button, .tabs button, .sidebar button")].find(b=>b.textContent.trim()==="Study")?.click()`);
  await sleep(1200);
  const hasView = await ev(`Boolean(document.querySelector(".study-view"))`);
  const created = await click(".study-new");
  await sleep(1500);
  const studies = await ev(`document.querySelectorAll(".study-row").length`);

  await typeInto(".chapter-title", "Openings");
  await click(".chapter-add");
  await sleep(1400);
  await typeInto(".chapter-title", "Tactics");
  await click(".chapter-add");
  await sleep(1400);
  const chapters = await ev(`[...document.querySelectorAll(".chapter-row .chapter-name")].map(c=>c.textContent.trim())`);

  const duplicated = await ev(`(()=>{const b=document.querySelector(".chapter-row .chapter-duplicate");if(!b)return false;b.click();return true;})()`);
  await sleep(1400);
  const afterDuplicate = await ev(`[...document.querySelectorAll(".chapter-row .chapter-name")].map(c=>c.textContent.trim())`);

  const moved = await ev(`(()=>{const rows=[...document.querySelectorAll(".chapter-row")];const last=rows[rows.length-1];const up=last.querySelector(".chapter-up");if(!up)return false;up.click();return true;})()`);
  await sleep(1400);
  const afterMove = await ev(`[...document.querySelectorAll(".chapter-row .chapter-name")].map(c=>c.textContent.trim())`);

  const boardAdded = await click(".strip-add");
  await sleep(1400);
  const boards = await ev(`document.querySelectorAll(".strip-board").length`);
  const primary = await click(".strip-primary");
  await sleep(1200);
  const primaryMarked = await ev(`[...document.querySelectorAll(".strip-board")].some(b=>b.textContent.includes("★"))`);

  await typeInto(".notes-editor", "## Notes for the chapter");
  const saved = await click(".notes-save");
  await sleep(1200);

  const published = await click(".study-publish");
  await sleep(1600);
  const lifecycle = await ev(`document.querySelector(".study-lifecycle")?.textContent.trim()`);
  const disabledAfter = await ev(`document.querySelector(".chapter-add")?.disabled`);

  console.log(JSON.stringify({ hasView, created, studies, chapters, duplicated, afterDuplicate, moved, afterMove, boardAdded, boards, primary, primaryMarked, saved, published, lifecycle, disabledAfter }));
  const ok = hasView && created && studies >= 1
    && chapters.length === 2 && chapters[0] === "Openings"
    && duplicated && afterDuplicate.length === 3 && afterDuplicate.some((t) => t.includes("copy"))
    && moved && afterMove.join() !== afterDuplicate.join()
    && boardAdded && boards >= 1 && primary && primaryMarked
    && saved && published && lifecycle === "published"
    && disabledAfter === true;
  console.log(ok ? "AI_STUDY_PASS" : "AI_STUDY_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
