// Session restore: a reload returns to the same tabs, and it still works after
// localStorage is cleared, because the client id is also kept in a cookie.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9538;
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-session-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1400,900", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await send("Page.enable"); await sleep(2800);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const tabs = async () => await ev(`document.querySelectorAll(".ws-tab").length`);

  // Open two workspace tabs through the strip, so there is session state that
  // would visibly disappear if restore failed.
  const created = await ev(`(()=>{
    const pick = document.querySelector("select.ws-new");
    if (!pick) return false;
    pick.value = "analysis";
    pick.dispatchEvent(new Event("change", {bubbles: true}));
    return true;
  })()`);
  await sleep(900);
  await ev(`(()=>{
    const pick = document.querySelector("select.ws-new");
    pick.value = "analysis";
    pick.dispatchEvent(new Event("change", {bubbles: true}));
    return true;
  })()`);
  await sleep(1600);
  const before = await tabs();
  const titlesBefore = await ev(`[...document.querySelectorAll(".ws-tab")].map(t=>t.textContent.trim())`);

  // The client id must exist in both places.
  const ids = await ev(`(()=>({
    local: localStorage.getItem('rbc.client'),
    cookie: (document.cookie.match(/rbc_client=([^;]+)/)||[])[1] || null,
  }))()`);

  // A plain reload keeps the session.
  await send("Page.reload", {}); await sleep(3200);
  const afterReload = await tabs();
  const titlesReload = await ev(`[...document.querySelectorAll(".ws-tab")].map(t=>t.textContent.trim())`);

  // Clearing localStorage must not lose it: the cookie carries the id.
  await ev(`localStorage.clear()`);
  await send("Page.reload", {}); await sleep(3400);
  const afterClear = await tabs();
  const titlesClear = await ev(`[...document.querySelectorAll(".ws-tab")].map(t=>t.textContent.trim())`);
  const restoredClient = await ev(`localStorage.getItem('rbc.client')`);

  // The reset notice is shown once per browser, then not again.
  await ev(`localStorage.removeItem('rbc.resetNotice.seen')`);
  await send("Page.reload", {}); await sleep(3200);
  const noticeFirst = await ev(`(()=>{const e=document.querySelector(".session-notice");return e?e.textContent.trim().slice(0,120):"";})()`);
  await send("Page.reload", {}); await sleep(3200);
  const noticeSecond = await ev(`(()=>{const e=document.querySelector(".session-notice");return e?e.textContent.trim().slice(0,120):"";})()`);

  console.log(JSON.stringify({ created, ids, before, titlesBefore, afterReload, titlesReload, afterClear, titlesClear, restoredClient, noticeFirst, noticeSecond }));
  const ok = created && before === 2 && ids && ids.local && ids.cookie
    && afterReload === 2 && afterClear === 2
    && JSON.stringify(titlesReload) === JSON.stringify(titlesBefore)
    && JSON.stringify(titlesClear) === JSON.stringify(titlesBefore)
    && !!restoredClient
    && /legacy_sessions/.test(noticeFirst) && noticeSecond === "";
  console.log(ok ? "AI_SESSION_PASS" : "AI_SESSION_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
