// Cross-board addressing: the shell knows which board the assistant is writing
// to, marks it, and points at it with a badge when it is off screen.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9539;
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-xb-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (sel, text) => await ev(`(()=>{const b=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(text)});if(!b)return false;b.click();return true;})()`);

  // A workspace with at least two boards, made through the strip itself: the
  // client's workspace list only knows about boards it created.
  await clickText(".nav", "Analysis");
  await sleep(1200);
  const setup = await ev(`(()=>{
    const pick = document.querySelector("select.ws-new");
    if (!pick) return false;
    pick.value = "analysis";
    pick.dispatchEvent(new Event("change", {bubbles: true}));
    return true;
  })()`);
  await sleep(1800);
  for (let i = 0; i < 3; i++) {
    const added = await ev(`(()=>{const b=[...document.querySelectorAll(".boards button")].find(x=>x.textContent.trim()==="New board");if(!b)return false;b.click();return true;})()`);
    if (!added) break;
    await sleep(1200);
  }

  // The Assistant needs a session so there is something to target.
  await clickText(".nav", "Assistant");
  await sleep(1200);
  await clickText("button", "New chat");
  await sleep(1500);

  // Back to Analysis, where the strip lives.
  await clickText(".nav", "Analysis");
  await sleep(1500);
  const chips = await ev(`[...document.querySelectorAll(".board-chip")].map(c=>c.textContent.trim())`);

  // Target the first board.
  const setTarget = await ev(`(()=>{const b=[...document.querySelectorAll(".board-set-target")];return b.length ? (b[0].click(), true) : false;})()`);
  await sleep(1400);
  const markedAfterSet = await ev(`[...document.querySelectorAll(".board-chip.target")].map(c=>c.textContent.trim())`);

  // Switch to the other board: the badge must point back at the target.
  const switched = await ev(`(()=>{const chips=[...document.querySelectorAll(".board-chip")];const other=chips.find(c=>!c.classList.contains("active"));if(!other)return false;other.click();return true;})()`);
  await sleep(1200);
  const badge = await ev(`(()=>{const e=document.querySelector(".board-target-badge");return e?e.textContent.trim():"";})()`);

  // Clicking the badge focuses the target board.
  const badgeClicked = await ev(`(()=>{const e=document.querySelector(".board-target-badge");if(!e)return false;e.click();return true;})()`);
  await sleep(1200);
  const badgeAfter = await ev(`!!document.querySelector(".board-target-badge")`);
  const activeChip = await ev(`(()=>{const c=document.querySelector(".board-chip.active");return c?c.textContent.trim():"";})()`);

  console.log(JSON.stringify({ setup, chips, setTarget, markedAfterSet, switched, badge, badgeClicked, badgeAfter, activeChip }));
  const ok = chips && chips.length >= 2 && setTarget && markedAfterSet.length === 1
    && /AI target/.test(markedAfterSet[0] || "")
    && switched && /AI → /.test(badge)
    && badgeClicked && badgeAfter === false
    && /AI target/.test(activeChip || "");
  console.log(ok ? "AI_CROSS_BOARD_PASS" : "AI_CROSS_BOARD_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
