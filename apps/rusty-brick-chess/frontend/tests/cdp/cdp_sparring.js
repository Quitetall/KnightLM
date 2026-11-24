// RBCUI-TRAIN-05. Use an actual engine and play from each side.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  if (!process.env.STOCKFISH_PATH) throw new Error("Set STOCKFISH_PATH for sparring verification");
  await destination("play");
  await clickText(".play button", "Play an engine");
  await view("practice");
  const fen = () => evaluate('document.querySelector(".practice .fen-line code")?.textContent');
  const status = () => evaluate('document.querySelector(".practice .engine-body .best")?.textContent');
  const move = async (from, to) => {
    for (const name of [from, to]) {
      const point = await evaluate(`(() => {
        const rect = document.querySelector('.practice [data-square="${name}"]')?.getBoundingClientRect();
        return rect && {x:rect.left + rect.width / 2, y:rect.top + rect.height / 2};
      })()`);
      if (!point) throw new Error("missing sparring square: " + name);
      for (const type of ["mousePressed", "mouseReleased"]) {
        await command("Input.dispatchMouseEvent", {type, ...point, button:"left", clickCount:1});
      }
    }
  };

  await setField(".practice-row select", "black", "change");
  await clickText(".practice-row button", "Start");
  await waitFor(async () => (await fen())?.includes(" b ") && (await status())?.includes("Your move"), "White engine did not play the first move");
  if (await evaluate('document.querySelector(".practice .board [data-square]")?.dataset.square') !== "h1") throw new Error("Black orientation missing");
  await move("e7", "e5");
  await waitFor(async () => (await fen())?.includes(" b ") && (await fen())?.endsWith(" 2") && (await status())?.includes("Your move"), "Black move did not receive an engine reply");

  const mate = "7k/6pp/5KQ1/8/8/8/8/8 w - - 0 1";
  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  await setField(".recorded-board .load input", mate);
  await click(".recorded-board .load button");
  await waitFor(() => evaluate(`document.querySelector(".recorded-board .fen-line code")?.textContent === "${mate}"`), "custom position did not load");
  await clickText(".detailed-analysis button", "Spar from here");
  await view("practice");
  await waitFor(() => evaluate(`document.querySelector(".practice-row input")?.value === "${mate}"`), "Analysis position was not transferred");
  await setField(".practice-row select", "white", "change");
  await clickText(".practice-row button", "Start");
  await waitFor(async () => await fen() === mate && (await status())?.includes("Your move"), "White custom game did not start");
  await move("g6", "g7");
  await waitFor(async () => (await status())?.includes("checkmate"), "mate did not report an outcome");
  await clickText(".practice .engine-body button", "New game");
  await waitFor(async () => await fen() === mate && (await status())?.includes("Your move"), "new game did not restore the position");
  console.log("RBC_SPARRING_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_SPARRING_FAIL");
  process.exitCode = 1;
});
