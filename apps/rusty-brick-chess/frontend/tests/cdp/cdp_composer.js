// The assistant composer: slash palette, @-mentions as chips, temperature, and
// regenerate. Each is checked by what a person would see happen.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9562;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=/tmp/chrome-composer-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", "http://127.0.0.1:8090/"], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;

  // A reference to mention: a study and its board, created through the API.
  const seeded = await ev(`(async () => {
    const study = await (await fetch("/api/studies", {method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({title:"Composer study"})})).json();
    await fetch("/api/studies/" + study.id + "/chapters", {method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({title:"A chapter"})});
    return Boolean(study.id);
  })()`);

  await ev(`[...document.querySelectorAll(".nav")].find(b=>b.textContent.trim()==="Assistant")?.click()`);
  await sleep(1600);
  // A conversation created the way a person does it, so the composer is enabled.
  const session = await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()==="New chat");if(!b)return false;b.click();return true;})()`);
  await sleep(2000);
  const hasComposer = await ev(`Boolean(document.querySelector(".composer"))`);

  // Slash palette: typing "/" lists commands; picking one fills the draft.
  await ev(`(()=>{const t=document.querySelector(".composer");t.focus();t.value="/";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);
  await sleep(600);
  const slashItems = await ev(`document.querySelectorAll(".composer-palette.slash .item").length`);
  const slashFirst = await ev(`document.querySelector(".composer-palette.slash .item")?.textContent.trim().slice(0,20)`);
  await ev(`document.querySelector(".composer-palette.slash .item")?.click()`);
  await sleep(500);
  const draftAfterSlash = await ev(`document.querySelector(".composer").value`);
  const paletteGone = await ev(`!document.querySelector(".composer-palette")`);

  // @-mention: typing "@" lists references; clicking one adds a chip.
  await ev(`(()=>{const t=document.querySelector(".composer");t.value="look at @";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);
  await sleep(800);
  const mentionItems = await ev(`document.querySelectorAll(".composer-palette.mention .item").length`);
  await ev(`(()=>{const i=document.querySelector(".composer-palette.mention .item");if(i)i.click();return true;})()`);
  await sleep(500);
  const chips = await ev(`document.querySelectorAll(".context-chip").length`);
  const chipText = await ev(`document.querySelector(".context-chip")?.textContent.trim().slice(0,30)`);
  await ev(`document.querySelector(".chip-remove")?.click()`);
  await sleep(400);
  const chipsAfterRemove = await ev(`document.querySelectorAll(".context-chip").length`);

  // Attachments: the current position as a FEN chip, and a pasted game.
  // A FEN typed into the composer is the position to attach; the fallback is
  // whatever board is on screen.
  const typeComposer = async (text) => {
    await ev(`(()=>{const t=document.querySelector(".composer");t.focus();t.value="";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);
    await sleep(200);
    await send("Input.insertText", { text });
    await sleep(300);
  };
  await typeComposer("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
  const fenTyped = await ev(`document.querySelector(".composer").value.slice(0,12)`);
  await ev(`document.querySelector(".attach-fen")?.click()`);
  await sleep(500);
  const fenChips = await ev(`[...document.querySelectorAll(".context-chip")].filter(c=>c.dataset.kind==="fen").length`);
  await typeComposer('[Event \"x\"] 1. e4 e5');
  const beforePgnClick = await ev(`document.querySelector(".composer").value`);
  await ev(`document.querySelector(".attach-pgn")?.click()`);
  await sleep(600);
  const allChips = await ev(`[...document.querySelectorAll(".context-chip")].map(c=>c.dataset.kind)`);
  const pgnChips = allChips.filter((k) => k === "pgn").length;
  const draftClearedByPgn = await ev(`document.querySelector(".composer").value === ""`);
  const hasImageInput = await ev(`Boolean(document.querySelector(".attach-image"))`);
  const hasModel = await ev(`Boolean(document.querySelector(".model-override"))`);

  // Temperature reaches the request: set 0.9 and send, watching the payload.
  // The client sends a Request object, so read the body from whichever shape
  // arrives rather than assuming fetch(url, init).
  await ev(`window.__sent=[]; const of_=window.fetch; window.fetch=function(u,o){
    try {
      if (u && typeof u === "object" && typeof u.clone === "function" && String(u.url||"").includes("/send")) {
        u.clone().text().then(t=>window.__sent.push(t)).catch(()=>{});
      } else if (String(u).includes("/send") && o && o.body) {
        window.__sent.push(String(o.body));
      }
    } catch (e) {}
    return of_.apply(this, arguments);
  };`);
  const tempSet = await ev(`(()=>{const r=document.querySelector(".temperature");if(!r)return false;r.value="0.9";r.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  await sleep(300);
  const tempLabel = await ev(`document.querySelector(".temperature-value")?.textContent.trim()`);
  await ev(`(()=>{const m=document.querySelector(".model-override");if(m){m.value="extra-model";m.dispatchEvent(new Event("input",{bubbles:true}));}})()`);
  await sleep(300);
  await ev(`(()=>{const t=document.querySelector(".composer");t.value="hello there";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);
  await sleep(300);
  const sendEnabled = await ev(`(()=>{const b=document.querySelector(".composer-send");return b?!b.disabled:false;})()`);
  await ev(`document.querySelector(".composer-send")?.click()`);
  await sleep(2500);
  await sleep(500);
  const sentBodies = await ev(`window.__sent.slice(0,1)`);
  const sentHasTemp = (sentBodies[0] || "").includes('"temperature":0.9');
  const sentHasModel = (sentBodies[0] || "").includes('"model":"extra-model"');
  const sentHasContext = (sentBodies[0] || "").includes("Context:");
  const hasRegenerate = await ev(`Boolean(document.querySelector(".composer-regenerate"))`);

  console.log(JSON.stringify({ seeded, session: Boolean(session), hasComposer, slashItems, slashFirst, draftAfterSlash: (draftAfterSlash||"").slice(0,24), paletteGone, mentionItems, chips, chipText, chipsAfterRemove, tempSet, tempLabel, sendEnabled, fenTyped, fenChips, allChips, beforePgnClick, pgnChips, draftClearedByPgn, hasImageInput, hasModel, sentHasTemp, sentHasModel, sentHasContext, sentBodies: (sentBodies||[]).map(b=>String(b).slice(0,160)), hasRegenerate }));
  const ok = seeded && Boolean(session) && hasComposer
    && slashItems >= 3 && paletteGone && (draftAfterSlash || "").length > 20
    && mentionItems >= 1 && chips === 1 && chipsAfterRemove === 0
    && tempSet && tempLabel === "0.9" && sendEnabled && sentHasTemp
    && fenChips === 1 && pgnChips === 1 && draftClearedByPgn && hasImageInput && hasModel
    && sentHasModel && sentHasContext
    && hasRegenerate;
  console.log(ok ? "AI_COMPOSER_PASS" : "AI_COMPOSER_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
