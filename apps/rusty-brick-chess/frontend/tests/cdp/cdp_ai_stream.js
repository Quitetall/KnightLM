const { openView } = require("./navigation");
// Streaming, stop and tool cards — self-contained.
//
// This script serves its own OpenAI-compatible SSE provider, so the check does
// not depend on an external mock staying alive. The app's server must be
// pointed at it: MODEL_URL=http://127.0.0.1:8100/v1/chat.
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9536);
const PROVIDER = Number(process.env.RBC_CDP_PROVIDER_PORT || 8100);
const CHUNK_DELAY_MS = 400;

// --- a provider that streams slowly and can call a tool --------------------

function startProvider() {
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      let payload = {};
      try { payload = JSON.parse(body || "{}"); } catch {}
      const messages = payload.messages || [];
      const sawTool = messages.some((message) => message.role === "tool");
      const user = ([...messages].reverse().find((message) => message.role === "user") || {}).content || "";
      res.writeHead(200, {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        connection: "keep-alive",
      });
      const send = (obj) => res.write("data: " + JSON.stringify(obj) + "\n\n");
      const chunk = (delta) => send({ id: "mock", choices: [{ index: 0, delta }] });
      const finish = (tokensIn, tokensOut) => {
        send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "stop" }] });
        send({ id: "mock", choices: [], usage: { prompt_tokens: tokensIn, completion_tokens: tokensOut } });
        res.write("data: [DONE]\n\n");
        res.end();
      };

      if (/board/i.test(user) && !sawTool) {
        // A tool round: ask for the boards, the server answers, we reply.
        chunk({ tool_calls: [{ index: 0, id: "call_1", function: { name: "list_boards", arguments: "{}" } }] });
        send({ id: "mock", choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] });
        send({ id: "mock", choices: [], usage: { prompt_tokens: 5, completion_tokens: 2 } });
        res.write("data: [DONE]\n\n");
        res.end();
        return;
      }

      if (user === "stall please") {
        chunk({content:"one chunk, then stalled"});
        return;
      }

      if (user === "stream please") {
        // Continue streaming until Stop so the browser has time to observe a
        // partial response. The provider checks cancellation between reads.
        chunk({ content: "the position is " });
        const interval = setInterval(() => chunk({ content: "quiet " }), CHUNK_DELAY_MS);
        res.on("close", () => clearInterval(interval));
        return;
      }

      chunk({reasoning_content:"Consider the legal alternatives."});
      const words = (sawTool ? "there are boards here" : "the position is equal and quiet").split(" ");
      let index = 0;
      const tick = () => {
        if (index < words.length) {
          chunk({ content: words[index++] + " " });
          setTimeout(tick, CHUNK_DELAY_MS);
          return;
        }
        finish(sawTool ? 11 : 9, sawTool ? 4 : 6);
      };
      tick();
    });
  });
  server.listen(PROVIDER, "127.0.0.1");
  return server;
}

function getJson(port, path) {
  return new Promise((resolve, reject) => {
    http.get({ host: "127.0.0.1", port, path }, (response) => {
      let data = "";
      response.on("data", (chunk) => (data += chunk));
      response.on("end", () => resolve(JSON.parse(data)));
    }).on("error", reject);
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

(async () => {
  const provider = startProvider();
  const chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${PORT}`, `--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-stream2-${Date.now()}`, "--remote-allow-origins=*", "--window-size=1500,1000", (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")], { stdio: ["ignore", "ignore", "ignore"] });
  let target;
  for (let i = 0; i < 40; i++) { await sleep(300); try { target = await getJson(PORT, "/json/list"); break; } catch {} }
  const ws = new WebSocket(target.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (method, params = {}) => new Promise((resolve) => { const i = ++id; pend.set(i, resolve); ws.send(JSON.stringify({ id: i, method, params })); });
  ws.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((resolve) => (ws.onopen = resolve));
  await send("Runtime.enable");
  await sleep(2800);
  const ev = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.value;
  const clickText = async (selector, text) => await ev(`(()=>{const e=[...document.querySelectorAll(${JSON.stringify(selector)})].find(x=>x.textContent.trim()===${JSON.stringify(text)});e&&e.click();return !!e;})()`);
  const type = async (text) => await ev(`(()=>{const t=document.querySelector(".chat-input textarea");if(!t)return false;t.value=${JSON.stringify(text)};t.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  const streamingText = async () => await ev(`(()=>{const b=document.querySelector(".chat-msg.streaming");return b?b.textContent.trim():"";})()`);
  const waitFor = async (read, ms = 8000) => { const start = Date.now(); while (Date.now() - start < ms) { const value = await read(); if (value) return value; await sleep(250); } return ""; };

  await openView(ev, "workbench", "assistant");
  await sleep(900);
  await clickText("button", "New chat");
  await sleep(1500);

  // --- streaming: text shows up before the turn ends ----------------------
  await type("stream please");
  await sleep(200);
  const started = await clickText(".chat-input button", "Send");
  const partial = await waitFor(streamingText, 12000);
  const labelWhileSending = await ev(`document.querySelector(".chat-input button")?.textContent.trim()`);

  // --- stop: the turn ends by request ------------------------------------
  const stopped = await clickText(".chat-input button", "Stop");
  const errorText = await waitFor(async () => await ev(`(()=>{const e=document.querySelector(".chat-msg.error");return e?e.textContent.trim():"";})()`), 8000);
  const labelAfterStop = await ev(`document.querySelector(".chat-input button")?.textContent.trim()`);

  await type("stall please");
  await clickText(".chat-input button", "Send");
  const stalledPartial = await waitFor(async () => String(await streamingText()).includes("one chunk, then stalled"), 8000);
  if (!stalledPartial) throw new Error("stalled provider did not emit its first chunk");
  await clickText(".chat-input button", "Stop");
  const stalledStopped = await waitFor(() => ev('document.querySelector(".chat-input button")?.textContent.trim() === "Send"'), 8000);
  if (!stalledStopped) throw new Error("Stop did not cancel a stalled provider stream within 8 seconds");

  // --- tool cards and usage: a stored turn with a tool result ------------
  await sleep(1000);
  await type("which boards do I have");
  await sleep(200);
  await clickText(".chat-input button", "Send");
  const cards = await waitFor(async () => await ev(`document.querySelectorAll(".tool-card").length`), 15000);
  const usage = await waitFor(async () => await ev(`[...document.querySelectorAll(".muted")].some(e=>/\\bin\\b/.test(e.textContent)&&/\\bout\\b/.test(e.textContent))`), 8000);

  await waitFor(() => ev('document.querySelector(".chat-input button")?.textContent.trim() === "Send"'), 10000);
  const reasoning = await waitFor(() => ev('!!document.querySelector(".chat-reasoning")'), 8000);
  await ev('document.querySelector(".chat-reasoning").open = true');
  const reasoningText = await ev('document.querySelector(".chat-reasoning").textContent');
  if (!String(reasoningText).includes("legal alternatives")) throw new Error("stored reasoning did not render");
  const countReplies = () => ev('document.querySelectorAll(".chat-msg.assistant").length');
  const beforeRegenerate = await countReplies();
  if (!await clickText(".composer-controls button", "Regenerate")) throw new Error("Regenerate missing");
  const regenerated = await waitFor(async () => await countReplies() === beforeRegenerate + 1
    && await ev('document.querySelector(".composer-send")?.textContent.trim() === "Send"'), 12000);
  await ev(`(() => {
    window.sentStreamSettings = [];
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const request = args[0] instanceof Request ? args[0].clone() : new Request(...args);
      if (new URL(request.url).pathname.endsWith("/send")) window.sentStreamSettings.push(await request.json());
      return original(...args);
    };
    const temperature=document.querySelector(".temperature");
    temperature.value="0.7"; temperature.dispatchEvent(new Event("input",{bubbles:true}));
    const model=document.querySelector(".model-override");
    model.value="fixture-custom"; model.dispatchEvent(new Event("input",{bubbles:true}));
  })()`);
  await type("normal response");
  await ev('document.querySelector(".composer").focus()');
  await send("Input.dispatchKeyEvent",{type:"keyDown",key:"Enter",code:"Enter",text:"\r",windowsVirtualKeyCode:13,modifiers:8});
  await send("Input.dispatchKeyEvent",{type:"keyUp",key:"Enter",code:"Enter",windowsVirtualKeyCode:13,modifiers:8});
  const multiline = await ev('document.querySelector(".composer").value.includes("\\n")');
  await send("Input.dispatchKeyEvent",{type:"keyDown",key:"Enter",code:"Enter",text:"\r",windowsVirtualKeyCode:13});
  await send("Input.dispatchKeyEvent",{type:"keyUp",key:"Enter",code:"Enter",windowsVirtualKeyCode:13});
  const keyboardSent = await waitFor(async () => await countReplies() === beforeRegenerate + 2
    && await ev('document.querySelector(".composer-send")?.textContent.trim() === "Send"'), 12000);
  const settings = await ev('window.sentStreamSettings[0]');
  if (!settings || settings.model !== "fixture-custom" || settings.temperature !== 0.7) throw new Error("composer settings were not sent");
  await ev('document.querySelector(".composer-knightlm").click()');
  const shortcut = await ev('document.querySelector(".model-override").value === "knightlm-smolvlm-500m" && document.querySelector(".composer-knightlm").getAttribute("aria-pressed") === "true"');
  const sessionId = await ev('localStorage.getItem("rbc.ai.session")');
  const beforePlay = await countReplies();
  await ev('document.querySelector(".composer-play").click()');
  const openedPlay = await waitFor(() => ev('document.querySelector("main")?.dataset.view === "play"'), 8000);
  await openView(ev,"workbench","assistant");
  const conversationRestored = await waitFor(async () => await countReplies() === beforePlay
    && await ev(`localStorage.getItem("rbc.ai.session") === ${JSON.stringify(sessionId)}`), 10000);
  if (!reasoning || !regenerated || !multiline || !keyboardSent || !shortcut || !openedPlay || !conversationRestored) {
    throw new Error(JSON.stringify({reasoning,regenerated,multiline,keyboardSent,shortcut,openedPlay,conversationRestored}));
  }
  const out = { started, partial: String(partial).slice(0, 40), labelWhileSending, stopped, errorText, labelAfterStop,
    stalledStopped, cards, usage, reasoning, regenerated, multiline, keyboardSent, shortcut, openedPlay, conversationRestored };
  console.log(JSON.stringify(out));
  const ok = started && String(partial).length > 0 && labelWhileSending === "Stop"
    && stopped && /cancel/i.test(String(errorText)) && labelAfterStop === "Send"
    && Number(cards) > 0 && usage === true;
  console.log(ok ? "AI_STREAM_PASS" : "AI_STREAM_FAIL");
  ws.close();
  chrome.kill();
  provider.close();
  process.exit(ok ? 0 : 1);
})().catch((error) => { console.error(error); process.exit(1); });
