// RBCUI-LIB-02. All writes go to the runner's disposable SQLite fixture.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view }) => {
  const library = async () => {
    await destination("workbench");
    await click('[data-open-view="database"]');
    await view("database");
  };
  const createGame = (white, move) => evaluate(`(async () => {
    const pgn = '[White "${white}"]\\n[Black "Opponent"]\\n[Result "*"]\\n\\n1. ${move} *';
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify({pgn}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  await view("home");
  const member = await createGame("Member", "e4");
  const other = await createGame("Outside", "d4");
  if (!member || !other) throw new Error("could not seed games");
  await library();
  const games = () => evaluate('[...document.querySelectorAll("[data-game-id]")].map(item => Number(item.dataset.gameId)).sort((a,b) => a-b).join(",")');
  const allGames = [member, other].sort((a,b) => a-b).join(",");
  await clickText(".collections button", "All games");
  await waitFor(async () => await games() === allGames, "all games list did not contain both fixtures");
  await setField('.collections input[placeholder="New collection…"]', "Parity set");
  await clickText(".collections button", "Create");
  const collection = () => evaluate('fetch("/api/collections").then(response => response.json()).then(rows => rows[0])');
  await waitFor(async () => (await collection())?.name === "Parity set", "collection creation did not persist");
  const id = (await collection()).id;
  await click(`[data-game-id="${member}"]`);
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  let addField;
  await waitFor(async () => {
    addField = await evaluate(`[...document.querySelectorAll(".detailed-analysis select")].findIndex(field =>
      [...field.options].some(option => option.textContent === "Parity set"))`);
    return addField >= 0;
  }, "collection missing from Analysis");
  await evaluate(`(() => {
    const field = document.querySelectorAll(".detailed-analysis select")[${addField}];
    field.value = "${id}";
    field.dispatchEvent(new Event("change", {bubbles:true}));
  })()`);
  await clickText(".detailed-analysis button", "Add");
  await waitFor(async () => (await collection())?.games === 1, "collection count did not update");

  await library();
  await waitFor(() => evaluate('[...document.querySelectorAll(".collections button")].some(button => button.textContent.trim() === "Parity set (1)")'), "Library did not refresh the new count");
  await clickText(".collections button", "Parity set (1)");
  await waitFor(async () => await games() === String(member), "collection filter included an unrelated game");
  const manage = 'select:has(option[value="0"]):has(option[value="' + id + '"])';
  await setField(manage, String(id), "change");
  await setField('input[placeholder="New name…"]', "Renamed set");
  await clickText(".dbs button", "Rename");
  await waitFor(async () => (await collection())?.name === "Renamed set", "rename did not persist");
  await waitFor(() => evaluate('document.querySelector(".collections .chip.active")?.textContent.includes("Renamed set (1)")'), "renamed collection lost selection or count");
  await evaluate("window.confirm = () => false");
  await clickText(".dbs button", "Delete");
  if ((await collection())?.id !== id) throw new Error("cancelled deletion removed the collection");
  await evaluate("window.confirm = () => true");
  await clickText(".dbs button", "Delete");
  await waitFor(async () => !(await collection()), "confirmed deletion did not remove the collection");
  await waitFor(async () => await games() === allGames, "deletion lost games or did not return to All games");
  console.log("RBC_LIBRARY_COLLECTIONS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_LIBRARY_COLLECTIONS_FAIL");
  process.exitCode = 1;
});
