// RBCUI-PLAY-07. Exercise a short game through the Analysis clock controls.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const importPgn = pgn => evaluate(`fetch("/api/games", {
    method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({pgn:${JSON.stringify(pgn)}}),
  }).then(response=>response.json()).then(data=>data.imported?.[0])`);
  const id = await importPgn('[White "Clock White"]\n[Black "Clock Black"]\n[Result "*"]\n[TimeControl "4+3"]\n\n1. e4 *');
  if (!id) throw new Error("clock fixture import failed");
  const open = async game => {
    await destination("workbench");
    await click('[data-open-view="database"]');
    await view("database");
    await click(".topbar button");
    await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${game}"]')`), "clock fixture not listed");
    await click(`[data-game-id="${game}"]`);
    await view("analysis");
    await click('[data-analysis-view="detailed"]');
  };
  const clocks = () => evaluate('[...document.querySelectorAll(".clock-chip")].map(chip=>chip.textContent.trim())');
  const active = () => evaluate('document.querySelector(".clock-chip.active")?.textContent.trim() || ""');
  const clockAction = name => clickText(".clock-controls button", name);
  const node = san => waitFor(() => evaluate(`(() => {
    const item = [...document.querySelectorAll(".tree .tnode")].find(button => button.textContent.includes(${JSON.stringify(san)}));
    if (!item) return false;
    item.click();
    return true;
  })()`), "recorded timed move missing");
  const square = async name => {
    const point = await evaluate(`(() => {
      const rect = document.querySelector('.recorded-board [data-square="${name}"]').getBoundingClientRect();
      return {x:rect.left + rect.width / 2,y:rect.top + rect.height / 2};
    })()`);
    for (const type of ["mousePressed","mouseReleased"]) {
      await command("Input.dispatchMouseEvent",{type,...point,button:"left",clickCount:1});
    }
  };

  await open(id);
  await waitFor(async () => (await clocks()).join("/") === "White 0:04/Black 0:04", "PGN time control did not initialize");
  await setField(".clock-controls input:first-of-type", "5");
  await setField(".clock-controls input:last-of-type", "2");
  await clockAction("Reset");
  await waitFor(async () => (await clocks()).join("/") === "White 0:05/Black 0:05", "reset did not apply base");
  if (!await evaluate('document.querySelectorAll(".clock-chip.low").length === 2')) throw new Error("low-time indication missing");
  await clockAction("Start");
  await waitFor(async () => (await active()).startsWith("White"), "White clock did not start");
  await clockAction("Pause");
  const paused = await clocks();
  await evaluate("new Promise(resolve => setTimeout(resolve, 1150))");
  if (await active() || (await clocks()).join("/") !== paused.join("/")) throw new Error("paused clock continued running");

  await clockAction("Reset");
  await square("d2");
  await square("d4");
  await waitFor(async () => (await clocks())[0] === "White 0:07" && (await active()).startsWith("Black"), "move did not add increment or change active side");
  const saved = () => evaluate(`fetch("/api/games/${id}/pgn").then(response=>response.text())`);
  await waitFor(async () => (await saved()).includes("[%clk 0:00:07]"), "recorded move did not keep increment in PGN");
  await node("d4");
  await waitFor(async () => await active() === "", "replay did not pause clock");
  await clockAction("Start");
  await waitFor(() => evaluate('document.querySelector(".topbar .status")?.textContent.includes("Black flag fell")'), "flag did not report");
  if ((await clocks())[1] !== "Black 0:00" || await active()) throw new Error("flagged clock did not stop at zero");

  const exported = await saved();
  const imported = await importPgn(exported.replace("Clock White", "Reimported Clock White"));
  if (!imported) throw new Error("clock PGN did not reimport");
  await open(imported);
  await node("d4");
  await waitFor(() => evaluate('document.querySelector(".tree .tnode.current")?.textContent.includes("0:00:07")'), "replayed move lost PGN clock");
  console.log("RBC_ANALYSIS_CLOCKS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ANALYSIS_CLOCKS_FAIL");
  process.exitCode = 1;
});
