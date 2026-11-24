// RBCUI-PLAY-08. Both entry routes, watch controls, and annotated notation.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view }) => {
  const openWatch = async (previous) => {
    await view("play");
    await click(".play-more summary");
    await setField(".play-opponent", "0", "change");
    await click(".play-watch");
    await waitFor(() => evaluate(`!!document.querySelector(".match-card.active")
      && document.querySelector(".match-card.active").dataset.matchId !== ${JSON.stringify(previous || "")}
      && !!document.querySelector(".match-step-5")`), "watch game did not open");
    return evaluate('document.querySelector(".match-card.active").dataset.matchId');
  };
  await view("home");
  await destination("workbench");
  await click('[data-open-view="engines"]');
  await view("engines");
  await clickText(".destination-grid button", "Play");
  const first = await openWatch();
  const count = () => evaluate('document.querySelectorAll(".match-moves .ply").length');
  await click(".match-step");
  await waitFor(async () => await count() === 1, "single step did not add exactly one ply");
  await click(".match-step-5");
  await waitFor(async () => await count() === 6, "five-step did not add exactly five plies");
  await click(".match-pause");
  await waitFor(() => evaluate('document.querySelector(".match-status")?.textContent.includes("paused")'), "pause did not reach the UI");
  await click(".match-step");
  await waitFor(() => evaluate('document.querySelector(".match-error")?.textContent.includes("paused")'), "paused step did not report refusal");
  const paused = await evaluate(`fetch("/api/ai/matches/${first}").then(response => response.json())`);
  if (paused.moves?.length !== 6) throw new Error("paused match advanced");
  await click(".match-resume");
  await waitFor(() => evaluate('!document.querySelector(".match-status")?.textContent.includes("paused")'), "resume did not reach the UI");
  await click(".match-step");
  await waitFor(async () => await count() === 7, "resumed match did not advance");
  await click(".match-handoff details summary");
  await waitFor(() => evaluate('document.querySelector(".match-pgn")?.textContent.includes("controls the centre")'), "watched PGN did not keep engine commentary");
  await click(".match-reset");
  await waitFor(async () => await count() === 0, "reset did not return to the start");
  await destination("home");
  await view("home");
  await destination("play");
  const second = await openWatch(first);
  if (first === second) throw new Error("second route did not create a new watch match");
  console.log("RBC_WATCH_CONTROLS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_WATCH_CONTROLS_FAIL");
  process.exitCode = 1;
});
