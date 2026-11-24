// RBCUI-LIB-01. Import through the actual Library controls and verify isolation.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  const selector = '.dbs:has(input[placeholder="New database…"]) select';
  const selected = () => evaluate(`document.querySelector(${JSON.stringify(selector)})?.value`);
  await waitFor(async () => !!await selected(), "initial database missing");
  const original = await selected();
  const names = () => evaluate('[...document.querySelectorAll("[data-game-id]")].map(row => row.children[1].textContent).sort().join(",")');
  const pgn = (name, move) => `[White "${name}"]\n[Black "Opponent"]\n[Result "*"]\n\n1. ${move} *`;
  await setField(".import textarea", pgn("Pasted", "e4"));
  await clickText(".import-actions button", "Import");
  await waitFor(async () => await names() === "Pasted", "pasted import did not appear");
  await setField('input[placeholder="New database…"]', "Parity second");
  await clickText('.dbs:has(input[placeholder="New database…"]) button', "Create");
  await waitFor(async () => !!await selected() && (await selected()) !== original && await names() === "", "new database was not selected and empty");
  const alternate = await selected();

  const files = [[pgn("FileA", "d4"), "file-a.pgn"], [pgn("FileB", "c4"), "file-b.pgn"]];
  const setFiles = (pairs, drop) => evaluate(`(() => {
    const transfer = new DataTransfer();
    for (const [text, name] of ${JSON.stringify(pairs)}) {
      transfer.items.add(new File([text], name, {type:"application/x-chess-pgn"}));
    }
    if (${drop}) {
      window.dispatchEvent(new DragEvent("drop", {dataTransfer: transfer, cancelable:true}));
    } else {
      const input = document.querySelector(".import input[type=file]");
      input.files = transfer.files;
      input.dispatchEvent(new Event("change", {bubbles:true}));
    }
  })()`);
  await setFiles(files, false);
  await waitFor(async () => await names() === "FileA,FileB", "multi-file import did not list both games");
  await setFiles([[pgn("Dropped", "Nf3"), "dropped.pgn"]], true);
  await waitFor(async () => await names() === "Dropped,FileA,FileB", "window drop did not import a game");
  await waitFor(() => evaluate(`["file-a.pgn","file-b.pgn","dropped.pgn"].every(name =>
    document.querySelector(".recent-files")?.textContent.includes(name))`), "recent file history missing");

  await setFiles([["[Result \"*\"]\n\n1. e5 *", "broken.pgn"]], true);
  await waitFor(() => evaluate('document.querySelector(".topbar .status")?.textContent.includes("broken.pgn")'), "invalid PGN did not show a scoped error");
  if (await names() !== "Dropped,FileA,FileB") throw new Error("invalid import changed the game list");
  await setField(selector, original, "change");
  await waitFor(async () => await selected() === original && await names() === "Pasted", "switching back lost original database");
  await setField(selector, alternate, "change");
  await waitFor(async () => await selected() === alternate && await names() === "Dropped,FileA,FileB", "switching forward lost imported games");
  const chosen = await evaluate('[...document.querySelectorAll("[data-game-id]")].find(row => row.children[1].textContent === "Dropped")?.dataset.gameId');
  await click(`[data-game-id="${chosen}"]`);
  await view("analysis");
  await waitFor(() => evaluate(`!!document.querySelector('a[href="/api/games/${chosen}/pgn"]')
    && document.querySelector(".ws-tab.active .ws-name")?.textContent.includes("Dropped")`), "wrong imported game opened");
  console.log("RBC_LIBRARY_IMPORT_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_LIBRARY_IMPORT_FAIL");
  process.exitCode = 1;
});
