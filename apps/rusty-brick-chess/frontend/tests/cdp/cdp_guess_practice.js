// RBCUI-TRAIN-04. Exercise both sources, legal wrong moves, reveal and reset.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const seed = (name, moves) => evaluate(`(async () => {
    const pgn = '[White "${name}"]\\n[Black "Opponent"]\\n[Result "*"]\\n\\n${moves} *';
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify({pgn}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  const primary = await seed("Practice fixture", "1. e4 e5");
  const alternate = await seed("Analysis source", "1. c4 e5");
  if (!primary || !alternate) throw new Error("could not seed practice fixtures");
  const practice = async () => {
    await destination("train");
    await click('[data-open-view="practice"]');
    await view("practice");
    await clickText(".practice button", "Guess the move");
  };
  const stats = () => evaluate('document.querySelector(".practice .engine-body")?.textContent');
  const fen = () => evaluate('document.querySelector(".practice .fen-line code")?.textContent');
  const playSquare = async name => {
    const point = await evaluate(`(() => {
      const rect = document.querySelector('.practice [data-square="${name}"]')?.getBoundingClientRect();
      return rect && {x:rect.left + rect.width / 2, y:rect.top + rect.height / 2};
    })()`);
    if (!point) throw new Error("practice square missing: " + name);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await command("Input.dispatchMouseEvent", {type, ...point, button:"left", clickCount:1});
    }
  };
  await practice();
  await setField(".practice-row input", "Practice fixture");
  await waitFor(() => evaluate(`[...document.querySelector(".practice-row select").options].map(option => option.value).join(",") === "0,${primary}"`), "game filter included the wrong game");
  await setField(".practice-row select", String(primary), "change");
  await clickText(".practice-row button", "Practise");
  await waitFor(async () => (await stats())?.includes("Move 1 / 2"), "saved game did not start");
  const start = await fen();
  await playSquare("d2");
  await playSquare("d4");
  await waitFor(async () => (await stats())?.includes("0 correct · 1 missed") && await fen() === start, "wrong move did not increment misses and restore position");
  await clickText(".practice .engine-body button", "Reveal");
  await waitFor(async () => (await stats())?.includes("e4"), "reveal did not show the next move");
  await playSquare("e2");
  await playSquare("e4");
  await waitFor(async () => (await stats())?.includes("1 correct · 1 missed") && (await stats())?.includes("Move 2 / 2") && await fen() !== start, "correct move did not advance and update score");
  await clickText(".practice .engine-body button", "Restart");
  await waitFor(async () => (await stats())?.includes("0 correct · 0 missed") && (await stats())?.includes("Move 1 / 2") && await fen() === start, "restart did not reset the drill");

  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await click(".topbar button");
  await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${alternate}"]')`), "second game not listed");
  await click(`[data-game-id="${alternate}"]`);
  await view("analysis");
  await practice();
  await clickText(".practice-row button", "Use the game in Analysis");
  await waitFor(async () => (await stats())?.includes("Play the game's move"), "current Analysis game did not start");
  await clickText(".practice .engine-body button", "Reveal");
  await waitFor(async () => (await stats())?.includes("c4") && await fen() === start, "current Analysis drill used the wrong record");
  console.log("RBC_GUESS_PRACTICE_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_GUESS_PRACTICE_FAIL");
  process.exitCode = 1;
});
