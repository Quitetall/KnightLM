// RBCUI-TRAIN-01. Import, filter, play, retry, and verify persisted attempts.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  await destination("train");
  await click('[data-open-view="puzzles"]');
  await view("puzzles");
  const start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
  const moves = "e2e4 e7e5 g1f3 b8c6";
  const csv = [
    "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags",
    `parityA,${start},${moves},1700,60,90,10,parity_fixture,,`,
    `parityB,${start},${moves},2500,60,90,10,parity_fixture,,`,
  ].join("\n");
  await setField(".import-csv", csv);
  await clickText(".engine-pane button", "Import CSV");
  await waitFor(() => evaluate('!!document.querySelector(".engine-pane select option[value=parity_fixture]")'), "imported theme not listed");
  await setField(".engine-pane select", "parity_fixture", "change");
  await setField(".engine-pane input[type=number]", "1700");
  await clickText(".engine-pane button", "Next puzzle");
  await waitFor(() => evaluate('document.querySelector(".engine-pane .engine-body")?.textContent.includes("Puzzle parityA")'), "rating/theme did not select nearest fixture");
  const fen = () => evaluate('document.querySelector(".analysis .fen-line code")?.textContent');
  const expectedStart = await fen();
  if (!expectedStart?.includes(" b ") || !expectedStart.includes("/4P3/")) throw new Error("setup move did not appear");
  await clickText(".engine-pane button", "Show solution");
  await waitFor(() => evaluate('document.querySelector(".engine-pane .pv")?.textContent.includes("Nc6")'), "solution did not display");
  await clickText(".engine-pane button", "Hide solution");
  if (await evaluate('!!document.querySelector(".engine-pane .pv")')) throw new Error("solution did not hide");

  const move = async (from,to) => {
    for (const square of [from,to]) {
      const point = await evaluate(`(() => {
        const box = document.querySelector('[data-square="${square}"]').getBoundingClientRect();
        return {x:box.left + box.width/2,y:box.top + box.height/2};
      })()`);
      for (const type of ["mousePressed","mouseReleased"]) {
        await command("Input.dispatchMouseEvent",{type,...point,button:"left",clickCount:1});
      }
    }
  };
  const attempts = () => evaluate('fetch("/api/puzzles/attempts?limit=8").then(response => response.json()).then(rows => rows.filter(row => row.puzzle_id === "parityA"))');
  await move("c7","c5");
  await waitFor(() => evaluate('document.querySelector(".puzzle-feedback.wrong")?.textContent.includes("Nc6")'), "wrong move did not reveal the solution");
  await waitFor(async () => (await attempts()).length === 1, "failed attempt not recorded");
  const failed = (await attempts())[0];
  if (failed.solved || failed.rating_after >= failed.rating_before) throw new Error("failed attempt did not lower rating");
  // A second move during the same try must not create a second score record.
  await move("g1","f3");
  if ((await attempts()).length !== 1) throw new Error("one try created multiple attempts");
  await clickText(".engine-pane button", "Retry");
  await waitFor(async () => await fen() === expectedStart, "retry did not restore setup");
  await move("e7","e5");
  await waitFor(async () => (await fen())?.includes(" b ") && (await fen())?.includes("/5N2/"), "correct move did not apply the opponent reply");
  await move("b8","c6");
  await waitFor(() => evaluate('document.querySelector(".puzzle-feedback.solved")?.textContent.includes("Rating")'), "solve and rating feedback missing");
  const scored = await attempts();
  if (scored.length !== 2 || !scored[0].solved || scored[0].rating_after <= scored[0].rating_before) throw new Error("solve rating or one-attempt-per-try rule failed");
  await waitFor(() => evaluate('document.querySelectorAll(".engine-pane .attempt").length === 2'), "attempt history did not update");
  console.log("RBC_PUZZLE_TRAINING_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_PUZZLE_TRAINING_FAIL");
  process.exitCode = 1;
});
