// RBCUI-AN-09. Exercise metadata and both repertoire imports from Detailed Analysis.
const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, evaluate, click, clickText, destination, setField, view }) => {
  const api = (path, body) => fetch(new URL("/api/" + path, base), {
    method: body ? "POST" : "GET",
    headers: {"content-type":"application/json"},
    body: JSON.stringify(body),
  });
  const json = async (...args) => {
    const response = await api(...args);
    assert.ok(response.ok, args[0] + " returned " + response.status);
    return response.json();
  };
  const seed = async (white, text) => (await json("games", {pgn:`[White "${white}"]\n[Black "Other"]\n[Result "*"]\n\n${text} *`})).imported[0];
  await view("home");
  const id = await seed("Fixture", "1. e4 (1. d4 d5 2. c4) e5 2. Nf3");
  const other = await seed("Keep", "1. c4");
  const collection = await json("collections", {name:"Analysis favorites"});
  const line = await json("repertoires", {name:"Line only",color:"white"});
  const tree = await json("repertoires", {name:"Full tree",color:"white"});
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await click(".topbar button");
  await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${id}"]')`), "seed game missing");
  await click(`[data-game-id="${id}"]`);
  await view("analysis");
  await click('[data-analysis-view="detailed"]');

  const action = text => clickText(".detailed-analysis button", text);
  await action("Edit tags");
  await action("Hide tags");
  assert.equal(await evaluate('!!document.querySelector(".tag-form")'), false);
  await action("Edit tags");
  const tags = {
    Event:"Edited event", Site:"Edited site", Date:"2024.09.13", Round:"4",
    White:"Edited White", Black:"Edited Black", Result:"1/2-1/2",
    WhiteElo:"1650", BlackElo:"1700", TimeControl:"180+2",
  };
  for (const [index, value] of Object.values(tags).entries()) {
    await setField(`.tag-form label:nth-child(${index + 1}) input`, value);
  }
  await action("Save tags");
  const game = () => json("games/" + id);
  await waitFor(async () => (await game()).white === tags.White, "tags did not save");
  const stored = await game();
  const keys = ["event","site","date","round","white","black","result","white_elo","black_elo","time_control"];
  assert.deepEqual(keys.map(key => String(stored[key])), Object.values(tags));
  const pgn = await (await api("games/" + id + "/pgn")).text();
  for (const [tag, value] of Object.entries(tags)) assert.ok(pgn.includes(`[${tag} "${value}"]`), "PGN lacks " + tag);
  await evaluate(`(() => {
    const link = [...document.querySelectorAll(".detailed-analysis a")].find(link=>link.textContent.trim()==="Save PGN");
    link.addEventListener("click", event => { event.preventDefault(); window.savedGameLink=link.href; });
  })()`);
  await click(".detailed-analysis a[download]");
  assert.equal(await evaluate("window.savedGameLink"), new URL("/api/games/" + id + "/pgn", base).href);

  const select = (label, value) => evaluate(`(() => {
    const select = [...document.querySelectorAll(".detailed-analysis select")]
      .find(field => field.options[0]?.textContent.includes(${JSON.stringify(label)}));
    if (!select) throw new Error("missing destination selector");
    select.value = ${JSON.stringify(String(value))};
    select.dispatchEvent(new Event("change", {bubbles:true}));
  })()`);
  await select("Add to collection", collection.id);
  await action("Add");
  await waitFor(async () => (await json("collections")).find(row=>row.id===collection.id)?.games === 1, "collection membership not saved");
  await evaluate(`[...document.querySelectorAll(".tree .tnode")].find(node=>node.textContent.includes("Nf3")).click()`);
  await select("Add line to", line.id);
  await action("Add line");
  const cards = async id => (await json("repertoires")).find(rep=>rep.id===id).cards;
  await waitFor(async () => await cards(line.id) === 2, "selected line did not yield two cards");
  await select("Add line to", tree.id);
  await action("Add whole game");
  await waitFor(async () => await cards(tree.id) === 4, "game tree did not include both branches");

  await evaluate("window.confirm = () => false");
  await action("Delete");
  assert.equal((await game()).id, id, "cancelled delete removed game");
  await evaluate("window.confirm = () => true");
  await action("Delete");
  await waitFor(async () => (await api("games/" + id)).status === 404, "confirmed game deletion did not persist");
  assert.equal((await json("games/" + other)).white, "Keep", "delete affected unrelated game");
  console.log("RBC_ANALYSIS_GAME_FILES_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ANALYSIS_GAME_FILES_FAIL");
  process.exitCode = 1;
});
