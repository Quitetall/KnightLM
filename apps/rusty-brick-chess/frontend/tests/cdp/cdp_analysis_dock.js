const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  await setField(".ws-new","analysis","change");
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  await clickText(".detailed-analysis button","AI chat");
  const action = text => clickText(".engine-dock button",text);
  const fen = () => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent');
  const before = await fen();
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const request=args[0] instanceof Request ? args[0].clone() : new Request(...args);
      const response=await original(...args);
      if (new URL(request.url).pathname === "/api/chat") {
        window.chatRequest=await request.json();
        window.chatReply=await response.clone().json();
        if (window.delayChat) await new Promise(resolve=>window.releaseChat=resolve);
      }
      return response;
    };
  })()`);
  for (const prompt of ["Analyse","Explain plan"]) {
    await evaluate("window.chatReply = null");
    await action(prompt);
    await waitFor(() => evaluate('!!window.chatReply && document.querySelectorAll(".engine-dock .tool-chip").length >= 4'),"dock tools did not run");
    assert.equal(await evaluate("window.chatRequest.fen"),before);
    await waitFor(() => evaluate('!!document.querySelector(".recorded-board .shapes line") && !!document.querySelector(".recorded-board .shapes circle")'),"assistant drawings missing");
    await action("Clear arrows");
    await waitFor(() => evaluate('!document.querySelector(".recorded-board .shapes line") && !document.querySelector(".recorded-board .shapes circle")'),"clear drawings failed");
  }
  await setField(".engine-dock textarea","Explain a different plan.");
  await action("Send");
  await waitFor(() => evaluate('document.querySelectorAll(".engine-dock .chat-msg.assistant").length === 3'),"free text reply missing");
  await action("Console");
  assert.equal(await evaluate('!!document.querySelector(".engine-dock .console-log")'),true);
  await action("AI");
  assert.equal(await evaluate('document.querySelectorAll(".engine-dock .chat-msg.assistant").length'),3);
  await click(".dock-close");
  assert.equal(await evaluate('!!document.querySelector(".engine-dock")'),false);
  await clickText(".detailed-analysis button","AI chat");
  assert.equal(await evaluate('document.querySelectorAll(".engine-dock .chat-msg.assistant").length'),3);

  await evaluate("window.delayChat = true");
  await setField(".engine-dock textarea","stale move");
  await action("Send");
  await waitFor(() => evaluate("!!window.releaseChat"),"delayed dock reply missing");
  await setField(".ws-new","analysis","change");
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  const other = await fen();
  await evaluate("window.releaseChat()");
  await evaluate("new Promise(resolve=>setTimeout(resolve,300))");
  assert.equal(await fen(),other,"old Assistant action changed the new workspace");
  console.log("RBC_ANALYSIS_DOCK_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_ANALYSIS_DOCK_FAIL");
  process.exitCode=1;
});
