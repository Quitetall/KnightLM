// RBCUI-NEW-01 and NEW-03. Exercise distinct destinations and both human seats.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, destination, setField, view, screenshot }) => {
  const text = selector => evaluate(`document.querySelector(${JSON.stringify(selector)})?.textContent.trim()`);
  const open = async name => { await destination(name); await view(name); };
  const choices = () => evaluate('[...document.querySelectorAll(".destination-hub [data-open-view]")].map(button => button.dataset.openView).join(",")');
  await view("home");
  if (!await evaluate('!!document.querySelector(".home") && !document.querySelector(".play")')) throw new Error("launch did not show the Home menu");
  await open("train");
  if (await choices() !== "puzzles,openings,study,practice") throw new Error("Train choices missing");
  await screenshot("train-desktop");
  await open("workbench");
  if (await choices() !== "analysis,database,engines,tournaments,assistant,compare,stats") throw new Error("Workbench tools missing");
  await screenshot("workbench-desktop");

  await open("play");
  await clickText(".play-head button", "Play an engine");
  await view("practice");
  await open("play");
  await click(".play-friend");
  await waitFor(async () => (await text(".match-status")) === "White to move", "local game did not start");
  const board = ".active-match .board";
  const move = async (from, to, next) => {
    for (const name of [from, to]) {
      const selector = board + ` [data-square="${name}"]`;
      const point = await evaluate(`(() => {
        const bounds = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect();
        return bounds && {x:bounds.left + bounds.width / 2, y:bounds.top + bounds.height / 2};
      })()`);
      if (!point) throw new Error("missing local game square: " + name);
      for (const type of ["mousePressed", "mouseReleased"]) {
        await command("Input.dispatchMouseEvent", {type, ...point, button:"left", clickCount:1});
      }
    }
    await waitFor(async () => (await text(".match-status")) === next, "local move did not advance: " + from + to);
  };
  await move("e2", "e4", "Black to move");
  await move("e7", "e5", "White to move");
  await evaluate('document.querySelector(".play-more").open = true');
  const matchId = await evaluate('document.querySelector(".match-card.active")?.dataset.matchId');
  if (!matchId) throw new Error("local game missing from Games");
  const match = await evaluate(`fetch("/api/ai/matches/" + ${JSON.stringify(matchId)}).then(r => r.json())`);
  if (match.white.kind !== "human" || match.black.kind !== "human" ||
      match.moves.map(move => move.san).join(",") !== "e4,e5") throw new Error("server did not record both human moves");
  if (!await evaluate(`(() => {
    const board = document.querySelector("${board}").getBoundingClientRect();
    const inspector = document.querySelector(".match-inspector").getBoundingClientRect();
    return board.width > inspector.width && board.right <= inspector.left;
  })()`)) throw new Error("Play board did not have priority at desktop width");
  await screenshot("play-desktop");

  for (const [kind, expected] of [["analysis","analysis"],["database","database"],["openings","openings"],["puzzles","puzzles"]]) {
    await setField(".ws-new", kind, "change");
    await view(expected);
  }
  await command("Emulation.setDeviceMetricsOverride", {width:390,height:844,deviceScaleFactor:1,mobile:true});
  await waitFor(() => evaluate(`(() => {
    const strip = document.querySelector(".workspaces");
    return innerWidth === 390 && strip.getBoundingClientRect().right <= innerWidth &&
      getComputedStyle(strip).overflowX === "auto" && strip.querySelectorAll(".ws-tab").length === 4;
  })()`), "workspace strip is clipped at narrow width");
  await evaluate('document.querySelector(".ws-tab .ws-name").scrollIntoView({block:"nearest",inline:"center"})');
  await click(".ws-tab .ws-name");
  await view("analysis");
  await open("play");
  await waitFor(async () => (await text(".match-moves"))?.includes("e5"), "local game did not resume from Analysis");
  if (!await evaluate(`(() => {
    const board = document.querySelector("${board}").getBoundingClientRect();
    const inspector = document.querySelector(".match-inspector").getBoundingClientRect();
    return board.top < inspector.top && board.left >= 0 && board.right <= innerWidth;
  })()`)) throw new Error("narrow Play should fit the board above its controls");
  await screenshot("play-narrow");

  await waitFor(() => evaluate(`!!document.querySelector('.play-opponent option[value="0"]')`), "configured model was unavailable");
  await setField(".play-opponent", "0", "change");
  await click(".play-start");
  await waitFor(async () => (await text(".match-status")) === "Your move", "model game did not open separately");
  await evaluate('document.querySelector(".play-more").open = true');
  await click(`[data-match-id="${matchId}"]`);
  await waitFor(async () => (await text(".match-status")) === "White to move" &&
    (await text(".match-moves"))?.includes("e5"), "local game did not resume independently of the model game");
  console.log("RBC_PLAY_LOCAL_LAYOUT_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_PLAY_LOCAL_LAYOUT_FAIL");
  process.exitCode = 1;
});
