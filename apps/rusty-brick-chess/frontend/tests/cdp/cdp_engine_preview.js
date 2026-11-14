// Exercise real engine lines, recorded position preservation, and stale responses.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view, screenshot }) => {
  await view("home");
  const engine = process.env.STOCKFISH_PATH;
  if (!engine) throw new Error("Set STOCKFISH_PATH to a local UCI engine for this suite");

  const game = await evaluate(`(async () => {
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"},
      body:JSON.stringify({pgn:'[White "Preview White"]\\n[Black "Preview Black"]\\n[Result "*"]\\n\\n1. e4 (1. d4 d5) e5 2. Nf3 *'}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  if (!game) throw new Error("could not seed recorded game");
  const savedPgn = () => evaluate(`fetch("/api/games/${game}/pgn").then(response => response.text())`);
  const before = await savedPgn();
  const settings = await evaluate(`(async () => {
    const path = ${JSON.stringify(engine)};
    const response = await fetch("/api/settings", {
      method:"PUT", headers:{"content-type":"application/json"},
      body:JSON.stringify({engine_path:path, depth:5, engines:[{name:"Preview fixture",path,options:{}},{name:"Second fixture",path,options:{}}]}),
    });
    return response.ok;
  })()`);
  if (!settings) throw new Error("could not configure fixture engine");
  // Reload loads the fixture depth and engine settings through the app's normal path.
  await evaluate("location.reload(); true");
  await view("home");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${game}"]')`), "game not listed");
  await click(`[data-game-id="${game}"]`);
  await view("analysis");
  const mode = name => click(`[data-analysis-view="${name}"]`);
  const boardFen = () => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent');
  const nextRecorded = () => click('.recorded-board button[title="Next"]');
  const engineButton = label => evaluate(`(() => {
    const button = [...document.querySelectorAll(".detailed-analysis button")]
      .find(item => item.textContent.trim() === ${JSON.stringify(label)});
    if (!button || button.disabled) return false;
    button.click();
    return true;
  })()`);
  const position = await boardFen();
  await nextRecorded();
  await waitFor(async () => (await boardFen())?.includes(" b "), "could not select the recorded e4 ply");
  const origin = await boardFen();
  if (origin === position) throw new Error("fixture did not advance");
  await waitFor(() => evaluate('document.querySelector(".analysis")?.dataset.analysisMode === "simple"'), "review did not open in Simple");
  await mode("detailed");
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const request = args[0] instanceof Request ? args[0].clone() : new Request(...args);
      const analyze = new URL(request.url).pathname === "/api/analyze";
      if (analyze) window.lastAnalyze = await request.clone().json();
      const response = await original(...args);
      if (analyze && !window.releaseSingle) await new Promise(resolve => { window.releaseSingle = resolve; });
      return response;
    };
  })()`);
  await waitFor(() => engineButton("Analyze"), "could not start single analysis");
  await waitFor(() => evaluate('!!window.releaseSingle && [...document.querySelectorAll(".detailed-analysis button")].some(button => button.disabled && button.textContent.includes("Analyzing"))'), "analysis busy state missing");
  const analyzed = await evaluate("window.lastAnalyze");
  if (analyzed.fen !== origin || analyzed.variant !== "standard") throw new Error("wrong analyzed FEN or variant");
  await evaluate("window.releaseSingle()");
  await waitFor(() => evaluate(`!!document.querySelector(".detailed-analysis .engine-body .preview-engine-line")
    && /depth \\d+/.test(document.querySelector(".detailed-analysis .eval-card")?.textContent)
    && /\\d+ nodes/.test(document.querySelector(".detailed-analysis .engine-body")?.textContent)`), "single engine score, depth, or nodes missing");
  await waitFor(() => engineButton("Lines"), "could not start MultiPV");
  await waitFor(() => evaluate('document.querySelectorAll(".lines .preview-engine-line").length === 3'), "three MultiPV lines missing");
  await waitFor(() => engineButton("All engines"), "could not start all-engine analysis");
  await waitFor(() => evaluate('document.querySelector(".engine-group .panel-sub")?.textContent.includes("Preview fixture")'), "named engine output missing");
  await waitFor(() => evaluate('[...document.querySelectorAll(".engine-group .panel-sub")].some(group => group.textContent.includes("Second fixture"))'), "second engine output missing");
  await click(".engine-group .preview-engine-line");
  const previewBoard = ".engine-line-preview .board";
  await waitFor(() => evaluate(`!!document.querySelector("${previewBoard}") && document.querySelector(".recorded-board").hidden`), "preview did not replace the visible board");
  const previewSquares = () => evaluate(`[...document.querySelectorAll("${previewBoard} [data-square]")].map(square => square.getAttribute("aria-label")).join("|")`);
  const recordedSquares = await evaluate('[...document.querySelectorAll(".recorded-board [data-square]")].map(square => square.getAttribute("aria-label")).join("|")');
  await waitFor(async () => await previewSquares() !== recordedSquares, "first preview move was not applied");
  await click(".engine-line-preview button:nth-child(2)");
  await waitFor(async () => await previewSquares() === recordedSquares, "preview start does not match its source");
  await click(".engine-line-preview button:nth-child(3)");
  await waitFor(async () => await previewSquares() !== recordedSquares, "preview step did not apply");
  await screenshot("engine-line-preview");
  await click(".engine-line-preview button:first-child");
  await waitFor(() => evaluate('!document.querySelector(".engine-line-preview") && !document.querySelector(".recorded-board").hidden'), "return to game did not restore original view");
  if (await boardFen() !== origin || await savedPgn() !== before) throw new Error("preview changed recorded position or PGN");
  await click(".engine-group .preview-engine-line");
  await mode("simple");
  await mode("detailed");
  if (await boardFen() !== origin || await evaluate('!!document.querySelector(".engine-line-preview")')) throw new Error("mode switch did not exit preview at the same ply");

  // Delay a real response after the server has computed it. Move to another ply
  // before releasing it, then ensure the old PV cannot reappear at that ply.
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const result = await original(...args);
      if (new URL(args[0].url || args[0], location.href).pathname === "/api/analyze/lines") {
        await new Promise(resolve => { window.releaseOldLine = resolve; });
      }
      return result;
    };
  })()`);
  await waitFor(() => engineButton("Lines"), "could not request delayed line");
  await waitFor(() => evaluate('!!window.releaseOldLine'), "server did not return delayed line");
  await nextRecorded();
  await waitFor(async () => (await boardFen()) !== origin, "did not navigate from the analyzed ply");
  await evaluate("window.releaseOldLine()");
  // Allow the released promise and reactive update to complete.
  await evaluate("new Promise(resolve => setTimeout(resolve, 400))");
  if (await evaluate('!!document.querySelector(".lines .preview-engine-line")')) throw new Error("stale result appeared at another ply");
  const selectedNode = () => evaluate('document.querySelector(".tree .tnode.current")?.textContent.trim()');
  const chosenVariation = await evaluate(`(() => {
    const move = [...document.querySelectorAll(".tree .tnode.variation")].find(node => /\\bd4\\b/.test(node.textContent));
    if (!move) return false;
    move.click();
    return true;
  })()`);
  if (!chosenVariation) throw new Error("recorded variation missing");
  await waitFor(async () => (await selectedNode())?.includes("d4"), "variation did not become selected");
  const variation = await boardFen();
  await mode("simple");
  await mode("detailed");
  if (!(await selectedNode())?.includes("d4") || await boardFen() !== variation) throw new Error("mode toggle lost selected variation");
  if (await savedPgn() !== before || !await evaluate(`!!document.querySelector('a[href="/api/games/${game}/pgn"]')`)) throw new Error("review lost the saved game");

  await engineButton("Engine console");
  await setField(".engine-dock select", engine, "change");
  await clickText(".engine-dock button", "Start");
  await waitFor(() => evaluate('!![...document.querySelectorAll(".engine-dock button")].find(button => button.textContent.trim() === "Stop" && !button.disabled)'), "engine console did not start");
  await setField(".engine-dock input", "isready");
  await evaluate(`document.querySelector(".engine-dock input").dispatchEvent(new KeyboardEvent("keydown", {key:"Enter",bubbles:true}))`);
  await waitFor(() => evaluate('document.querySelector(".console-log")?.textContent.includes("readyok")'), "UCI Enter command did not return readyok");
  await setField(".engine-dock input", "go depth 2");
  await clickText(".engine-dock button", "Send");
  await waitFor(() => evaluate('document.querySelector(".console-log")?.textContent.includes("bestmove")'), "UCI search did not return a move");
  await mode("simple");
  await mode("detailed");
  if (!await evaluate('document.querySelector(".console-log")?.textContent.includes("bestmove")')) throw new Error("console lost its transcript when switching modes");
  await clickText(".engine-dock button", "Stop");
  await waitFor(() => evaluate('document.querySelector(".engine-dock .status")?.textContent.trim() === "stopped"'), "console did not stop");
  const variantId = await evaluate(`(async () => {
    const pgn = '[White "Variant White"]\\n[Black "Variant Black"]\\n[Variant "Chess960"]\\n[Result "*"]\\n\\n1. e4 *';
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify({pgn}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  if (!variantId) throw new Error("could not seed Chess960 game");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await click(".topbar button");
  await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${variantId}"]')`), "variant game missing");
  await click(`[data-game-id="${variantId}"]`);
  await view("analysis");
  await mode("detailed");
  await waitFor(() => engineButton("Analyze"), "could not start Chess960 analysis");
  await waitFor(() => evaluate('window.lastAnalyze?.variant === "chess960" && !!document.querySelector(".detailed-analysis .engine-body .preview-engine-line")'), "variant engine request or output missing");
  await click(".detailed-analysis .engine-body .preview-engine-line");
  await waitFor(() => evaluate('!!document.querySelector(".engine-line-preview .board")'), "Chess960 preview missing");
  await click(".engine-line-preview button:first-child");

  const engineRemoved = await evaluate(`fetch("/api/settings", {
    method:"PUT", headers:{"content-type":"application/json"},
    body:JSON.stringify({engine_path:"/rbc-parity-missing-engine", engines:[]}),
  }).then(response => response.ok)`);
  if (!engineRemoved) throw new Error("could not seed missing engine");
  await waitFor(() => engineButton("Analyze"), "could not probe missing engine");
  await waitFor(() => evaluate('document.querySelector(".topbar .status")?.textContent.toLowerCase().includes("engine")'), "missing engine error not visible");
  console.log("RBC_ENGINE_PREVIEW_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ENGINE_PREVIEW_FAIL");
  process.exitCode = 1;
});
