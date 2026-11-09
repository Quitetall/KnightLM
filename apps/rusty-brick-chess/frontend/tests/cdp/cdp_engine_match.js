// RBCUI-PLAY-09. Real engines, both seats, pause with a reply in flight, and mate.
const { withBrowser, waitFor } = require("./browser");
const { writeFileSync, readFileSync } = require("node:fs");
const { join } = require("node:path");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  const engine = process.env.STOCKFISH_PATH;
  if (!engine || !process.env.TMPDIR) throw new Error("use the isolated runner with STOCKFISH_PATH");
  const paths = ["white", "black"].map(side => join(process.env.TMPDIR, "engine-" + side));
  for (const path of paths) {
    writeFileSync(path, `#!/usr/bin/env python3
import subprocess
import sys

with open(${JSON.stringify(path + ".uci")}, "ab", buffering=0) as log:
    with subprocess.Popen([${JSON.stringify(engine)}], stdin=subprocess.PIPE) as child:
        for line in sys.stdin.buffer:
            log.write(line)
            child.stdin.write(line)
            child.stdin.flush()
            if line.strip() == b"quit":
                break
        child.stdin.close()
`, {mode:0o700});
  }
  const settings = paths.map((path, i) => ({path, name:i ? "Black fixture" : "White fixture", options:{Hash:i ? "24" : "16", Threads:"1"}}));
  await view("home");
  const configured = await evaluate(`fetch("/api/settings",{method:"PUT",headers:{"content-type":"application/json"},
    body:JSON.stringify({engine_path:${JSON.stringify(paths[0])},engines:${JSON.stringify(settings)}})}).then(response=>response.ok)`);
  if (!configured) throw new Error("engine fixtures not configured");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector("main")?.dataset.view === "home"'), "app did not reload");
  await destination("workbench");
  await click('[data-open-view="engines"]');
  await view("engines");
  await clickText(".destination-grid button", "Engine match");
  await view("practice");
  await waitFor(() => evaluate('document.querySelector(".practice-row select")?.textContent.includes("White fixture")'), "engine selectors did not load");
  const row = ".practice .practice-row";
  const fen = () => evaluate('document.querySelector(".practice .fen-line code")?.textContent');
  const moves = () => evaluate('document.querySelector(".practice .match-moves")?.textContent.trim()');
  const status = () => evaluate('document.querySelector(".practice .practice-stats")?.textContent.trim()');
  const start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
  const mate = "7k/6pp/5KQ1/8/8/8/8/8 w - - 0 1";
  const selectEngines = async () => {
    await setField(row + " select:nth-child(1)", "0", "change");
    await setField(row + " select:nth-child(2)", "1", "change");
    await setField(row + " input[type=number]", "4");
  };
  await selectEngines();
  await setField(row + " input[type=text]", start);
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.engineRequests = [];
    window.engineResponses = [];
    window.fetch = async (...args) => {
      const request = args[0] instanceof Request ? args[0].clone() : new Request(...args);
      if (new URL(request.url).pathname !== "/api/analyze/engine") return original(...args);
      window.engineRequests.push(await request.json());
      const index = window.engineRequests.length;
      const response = await original(...args);
      window.engineResponses.push([response.status, await response.clone().text()]);
      if (index === 3) await new Promise(resolve => { window.releaseEngine = resolve; });
      return response;
    };
  })()`);
  await clickText(row + " button", "Start");
  try {
    await waitFor(() => evaluate('!!window.releaseEngine'), "engine match did not advance both seats");
  } catch (error) {
    console.log(JSON.stringify({status:await status(), fen:await fen(), requests:await evaluate("window.engineRequests"), replies:await evaluate("window.engineResponses")}));
    throw error;
  }
  const requests = await evaluate("window.engineRequests");
  if (requests[0].fen !== start || requests[0].engine !== paths[0] ||
      requests[1].engine !== paths[1] || !requests[1].fen.includes(" b ") ||
      !requests.every(request => request.depth === 4)) throw new Error("wrong engine, FEN, or depth");
  for (const [index, path] of paths.entries()) {
    const uci = readFileSync(path + ".uci", "utf8");
    if (!uci.includes("setoption name Hash value " + (index ? "24" : "16")) || !uci.includes("setoption name Threads value 1")) {
      throw new Error("seat-specific UCI settings were not applied");
    }
  }
  const current = await fen();
  const moveList = await moves();
  if (!moveList || current === start) throw new Error("live notation and position did not advance");
  await clickText(row + " button", "Pause");
  await waitFor(async () => await status() === "paused", "pause did not set status");
  await evaluate("window.releaseEngine()");
  await evaluate("new Promise(resolve => setTimeout(resolve, 350))");
  if (await fen() !== current || await moves() !== moveList || await status() !== "paused") throw new Error("in-flight reply mutated the paused game");
  await clickText(row + " button", "Reset");
  await waitFor(async () => await fen() === start && await moves() === "", "reset did not restore initial position");

  await destination("train");
  await click('[data-open-view="practice"]');
  await view("practice");
  await clickText(".practice button", "Engine match");
  await selectEngines();
  await setField(row + " input[type=text]", mate);
  await clickText(row + " button", "Start");
  await waitFor(async () => (await status())?.includes("checkmate") && (await status())?.includes("1-0"), "terminal position was not scored");
  if (!(await moves())?.includes("#")) throw new Error("mating move not shown");
  console.log("RBC_ENGINE_MATCH_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ENGINE_MATCH_FAIL");
  process.exitCode = 1;
});
