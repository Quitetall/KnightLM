// RBCUI-PLAY-06. Play a real engine from Analysis with both colors.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, setField, view }) => {
  await view("home");
  if (!process.env.STOCKFISH_PATH) throw new Error("STOCKFISH_PATH is required");
  if (!await evaluate('fetch("/api/settings",{method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({depth:5})}).then(response=>response.ok)')) {
    throw new Error("engine depth fixture failed");
  }
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector("main")?.dataset.view === "home"'), "app did not reload");
  const newAnalysis = async () => {
    await setField(".ws-new", "analysis", "change");
    await view("analysis");
    await click('[data-analysis-view="detailed"]');
  };
  const board = ".recorded-board";
  const fen = () => evaluate(`document.querySelector("${board} .fen-line code")?.textContent`);
  const start = "8/8/8/8/8/2k5/5R2/7K w - - 0 1";
  const load = async () => {
    await setField(board + " .load input", start);
    await click(board + " .load button");
    await waitFor(async () => await fen() === start, "custom FEN did not load");
  };
  const toggle = async enabled => evaluate(`(() => {
    const label = [...document.querySelectorAll(".detailed-analysis label")].find(label => label.textContent.trim() === "Play vs engine");
    const check = label?.querySelector("input");
    if (!check) throw new Error("engine opponent checkbox missing");
    if (check.checked !== ${enabled}) check.click();
  })()`);
  const side = value => setField('.detailed-analysis select:has(option[value="black"]):has(option[value="white"])', value, "change");
  const square = async name => {
    const point = await evaluate(`(() => {
      const box = document.querySelector('${board} [data-square="${name}"]').getBoundingClientRect();
      return {x:box.left + box.width / 2,y:box.top + box.height / 2};
    })()`);
    for (const type of ["mousePressed","mouseReleased"]) {
      await command("Input.dispatchMouseEvent",{type,...point,button:"left",clickCount:1});
    }
  };
  await newAnalysis();
  await load();
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.replies = [];
    window.pending = [];
    window.fetch = async (...args) => {
      const request = args[0] instanceof Request ? args[0].clone() : new Request(...args);
      if (new URL(request.url).pathname !== "/api/analyze") return original(...args);
      const payload = await request.json();
      const index = window.pending.push(payload);
      const response = await original(...args);
      window.replies[index] = await response.clone().json();
      if (index === 1 || index === 3) await new Promise(resolve => { window.releaseReply = resolve; });
      return response;
    };
  })()`);

  await side("white");
  await toggle(true);
  await square("f2");
  await square("f3");
  await waitFor(() => evaluate('typeof window.releaseReply === "function" && document.querySelector(".detailed-analysis")?.textContent.toLowerCase().includes("thinking")'), "engine did not enter thinking state");
  const waiting = await fen();
  if (!waiting?.includes(" b ")) throw new Error("white move did not hand the turn to engine");
  await square("c3");
  await square("c4");
  if (await fen() !== waiting) throw new Error("human moved during the engine turn");
  const whiteRequest = await evaluate("window.pending[0]");
  if (whiteRequest?.depth !== 5 || whiteRequest?.fen !== waiting) throw new Error("engine settings or requested position changed");
  await evaluate("window.releaseReply(); window.releaseReply = undefined");
  await waitFor(async () => (await fen())?.includes(" w ") && await fen() !== waiting, "black engine reply did not apply");
  if (!await evaluate('!!window.replies[1]?.best_move')) throw new Error("black engine returned no legal reply");

  await toggle(false);
  await load();
  await side("black");
  await toggle(true);
  await waitFor(async () => (await fen())?.includes(" b ") && await fen() !== start, "white engine did not open for a black player");
  if (await evaluate(`document.querySelector("${board} .board [data-square]")?.dataset.square`) !== "h1") throw new Error("black perspective missing");

  await toggle(false);
  await load();
  await toggle(true);
  await waitFor(() => evaluate('window.pending.length === 3 && typeof window.releaseReply === "function"'), "delayed opener not captured");
  await newAnalysis();
  const other = await fen();
  await evaluate("window.releaseReply()");
  await evaluate("new Promise(resolve => setTimeout(resolve, 350))");
  if (await fen() !== other || !other?.includes(" w ")) throw new Error("late engine reply changed a different workspace");
  console.log("RBC_ANALYSIS_ENGINE_PLAY_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ANALYSIS_ENGINE_PLAY_FAIL");
  process.exitCode = 1;
});
