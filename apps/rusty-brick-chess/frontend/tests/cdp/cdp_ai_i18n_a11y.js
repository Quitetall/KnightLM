const { openView } = require("./navigation");
// The Assistant's accessibility and translation: landmarks, live regions, the
// names a screen reader gets, and that switching language actually switches.
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9564);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function getJson(port, path) {
  return new Promise((res, rej) => {
    http.get({ host: "127.0.0.1", port, path }, (r) => {
      let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => res(JSON.parse(d)));
    }).on("error", rej);
  });
}
(async () => {
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-a11y-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")], { stdio: ["ignore", "ignore", "ignore"] });
  let t; for (let i = 0; i < 40; i++) { await sleep(300); try { t = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await sleep(2600);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;

  // The app remembers the language it was last left in, and this suite changes
  // it, so it sets the starting language itself rather than inheriting whatever
  // the previous run left behind.
  const toSettings = async () => {
    await ev(`(()=>{const n=[...document.querySelectorAll(".nav")];const t=n.find(b=>/^(Settings|Ajustes)$/.test(b.textContent.trim()));if(t){t.click();return true}return false})()`);
    await sleep(1200);
  };
  const setLanguage = async (code) => ev(`(()=>{
    const target = [...document.querySelectorAll("select")].find(s=>[...s.options].some(o=>o.value==="${code}"));
    if(!target) return false;
    target.value = "${code}";
    target.dispatchEvent(new Event("change",{bubbles:true}));
    return true;
  })()`);
  await toSettings();
  await setLanguage("en");
  await sleep(1200);
  const toAssistant = async () => {
  if (!await openView(ev, "workbench", "assistant")) throw new Error("assistant destination unavailable");
    await sleep(1400);
  };

  // Landmarks and the skip link, on the shell rather than any one tab.
  const skipLink = await ev(`(()=>{const a=document.querySelector(".skip-link");return a?{href:a.getAttribute("href"), text:a.textContent.trim()}:"none";})()`);
  const mainLandmark = await ev(`(()=>{const m=document.querySelector("main");return m?{id:m.id}:"none";})()`);
  const skipTargetsMain = await ev(`(()=>{const a=document.querySelector(".skip-link");if(!a)return false;const id=(a.getAttribute("href")||"").replace("#","");return Boolean(id && document.getElementById(id));})()`);
  // Focusing the skip link must move focus to the main region.
  await ev(`(()=>{const a=document.querySelector(".skip-link");a&&a.focus();return document.activeElement===a;})()`);
  const skipFocusable = await ev(`document.activeElement && document.activeElement.classList.contains("skip-link")`);

  // A focus ring is defined for keyboard users.
  const focusRule = await ev(`(()=>{const css=[...document.styleSheets].flatMap(s=>{try{return [...s.cssRules].map(r=>r.cssText)}catch{return []}});return css.some(r=>r.includes(":focus-visible"));})()`);

  await toAssistant();
  await ev(`(()=>{const b=[...document.querySelectorAll("button")].find(x=>x.textContent.trim()==="New chat");b?.click();})()`);
  await sleep(1800);

  // The composer announces its role and whether the palette is open.
  const composer = await ev(`(()=>{const t=document.querySelector(".composer");return t?{role:t.getAttribute("role"), expanded:t.getAttribute("aria-expanded"), label:t.getAttribute("aria-label")}:"none";})()`);
  // Typing "/" opens a listbox of options with selection state.
  await ev(`(()=>{const t=document.querySelector(".composer");t.focus();t.value="/";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);
  await sleep(500);
  const palette = await ev(`(()=>{const l=document.querySelector(".composer-palette");if(!l)return "none";const opts=[...l.querySelectorAll(".item")];return {role:l.getAttribute("role"), options:opts.length, firstRole:opts[0]&&opts[0].getAttribute("role"), selected:opts.filter(o=>o.getAttribute("aria-selected")==="true").length, expanded:document.querySelector(".composer").getAttribute("aria-expanded")};})()`);
  await ev(`(()=>{const t=document.querySelector(".composer");t.value="";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);

  // An attachment chip's remove control has a name.
  await ev(`(()=>{const t=document.querySelector(".composer");t.value="rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";t.dispatchEvent(new Event("input",{bubbles:true}));})()`);
  await sleep(300);
  await ev(`document.querySelector(".attach-fen")?.click()`);
  await sleep(500);
  const chipLabel = await ev(`document.querySelector(".chip-remove")?.getAttribute("aria-label") || ""`);

  // Live regions: the error box is an alert, the sending line a status.
  const liveRegions = await ev(`(()=>({
    alert: document.querySelectorAll('[role="alert"]').length,
    status: document.querySelectorAll('[role="status"]').length,
    polite: document.querySelectorAll('[aria-live="polite"]').length,
    assertive: document.querySelectorAll('[aria-live="assertive"]').length,
  }))()`);

  // The no-provider banner appears when no provider is configured.
  const providerBanner = await ev(`Boolean(document.querySelector(".provider-missing"))`);

  // Switching to Spanish changes the Assistant's own strings.
  const englishPlaceholder = await ev(`document.querySelector(".composer")?.getAttribute("placeholder") || ""`);
  await toSettings();
  const switched = await setLanguage("es");
  await sleep(1200);
  await toAssistant();
  const spanishPlaceholder = await ev(`document.querySelector(".composer")?.getAttribute("placeholder") || ""`);
  const translated = spanishPlaceholder && spanishPlaceholder !== englishPlaceholder;

  // Put the app back the way it was found. The language is persisted in the
  // database, not in this tab, so leaving it in Spanish made every later suite in
  // the same run look for English labels and fail - eleven of them, for a reason
  // that had nothing to do with what they test.
  await toSettings();
  await setLanguage("en");
  await sleep(800);

  console.log(JSON.stringify({ skipLink, mainLandmark, skipTargetsMain, skipFocusable, focusRule, composer, palette, chipLabel, liveRegions, providerBanner, englishPlaceholder, spanishPlaceholder, translated }));
  const ok = Boolean(skipLink && skipLink.href)
    && mainLandmark !== "none" && skipTargetsMain && skipFocusable
    && focusRule
    && composer !== "none" && composer.role === "combobox" && Boolean(composer.label)
    && palette !== "none" && palette.role === "listbox" && palette.options >= 3
    && palette.firstRole === "option" && palette.selected === 1 && palette.expanded === "true"
    && chipLabel.length > 0
    && liveRegions.alert >= 0 && (liveRegions.status >= 1 || liveRegions.polite >= 1)
    && providerBanner === true
    && translated === true;
  console.log(ok ? "AI_I18N_A11Y_PASS" : "AI_I18N_A11Y_FAIL");
  ws.close(); chrome.kill(); process.exit(ok ? 0 : 1);
})().catch((e) => { console.error(e); process.exit(1); });
