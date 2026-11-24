// RBCUI-AN-02. Edit stored branches through the board and PGN editor.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const seed = (white, moves) => evaluate(`(async () => {
    const pgn = ${JSON.stringify(`[White "${white}"]\n[Black "Fixture"]\n[Result "*"]\n\n${moves} *`)};
    const response = await fetch("/api/games", {method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({pgn})});
    return (await response.json()).imported?.[0];
  })()`);
  const first = await seed("Branch fixture", "1. e4 e5 2. Nf3 Nc6");
  const second = await seed("Truncation fixture", "1. d4 d5 2. c4 e6");
  if (!first || !second) throw new Error("could not seed tree fixtures");
  const game = id => evaluate(`fetch("/api/games/${id}").then(response => response.json())`);
  const pgn = id => evaluate(`fetch("/api/games/${id}/pgn").then(response => response.text())`);
  const open = async id => {
    await destination("workbench");
    await click('[data-open-view="database"]');
    await view("database");
    await click(".topbar button");
    await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${id}"]')`), "tree fixture not listed");
    await click(`[data-game-id="${id}"]`);
    await view("analysis");
    await click('[data-analysis-view="detailed"]');
    await waitFor(() => evaluate(`!!document.querySelector('a[href="/api/games/${id}/pgn"]')`), "opened the wrong game");
  };
  const node = (san, variation = false) => waitFor(() => evaluate(`(() => {
    const entry = [...document.querySelectorAll(${JSON.stringify(".tree .tnode" + (variation ? ".variation" : ""))})]
      .find(button => button.textContent.trim().split(/\\s+/).includes(${JSON.stringify(san)}));
    if (!entry) return false;
    entry.click();
    return true;
  })()`), "tree node missing: " + san);
  const fen = () => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent');
  const action = text => clickText(".detailed-analysis button", text);
  const boardClick = async square => {
    const point = await evaluate(`(() => {
      const rect = document.querySelector('.recorded-board [data-square="${square}"]').getBoundingClientRect();
      return {x:rect.left + rect.width / 2, y:rect.top + rect.height / 2};
    })()`);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await command("Input.dispatchMouseEvent", {type,...point,button:"left",clickCount:1});
    }
  };
  await open(first);
  const record = '.detailed-analysis label.checkline:has(input)';
  await evaluate(`(() => {
    const label = [...document.querySelectorAll(${JSON.stringify(record)})].find(label => label.textContent.trim() === "Record moves");
    const input = label?.querySelector("input");
    if (!input) throw new Error("Record moves unavailable");
    if (!input.checked) input.click();
  })()`);
  await node("e4");
  await waitFor(async () => (await fen())?.includes(" b ") && (await fen())?.includes("/4P3/"), "e4 position not selected");
  await boardClick("c7");
  await boardClick("c5");
  await waitFor(async () => (await pgn(first)).includes("(1... c5)"), "recorded move did not become a branch");
  if ((await game(first)).moves !== "e4 e5 Nf3 Nc6") throw new Error("recording branch changed main line");

  await node("c5", true);
  await action("Promote variation");
  await waitFor(async () => (await game(first)).moves === "e4 c5" && (await pgn(first)).includes("e5 2. Nf3 Nc6"), "promote did not preserve previous main line");
  await node("e5", true);
  await action("Delete from here");
  await waitFor(async () => (await game(first)).moves === "e4 c5" && !(await pgn(first)).includes("e5"), "delete did not remove only selected branch");

  await open(second);
  await node("d5");
  await waitFor(async () => (await fen())?.includes(" w ") && (await fen())?.includes("/3p4/"), "d5 position not selected");
  await action("Keep up to current move");
  await waitFor(async () => (await game(second)).moves === "d4 d5", "truncation did not retain exactly the selected ply");
  await action("Edit moves");
  const editor = ".detailed-analysis .pgn-box textarea";
  await waitFor(() => evaluate(`document.querySelector(${JSON.stringify(editor)})?.value.includes("d4 d5")`), "editor did not load recorded PGN");
  await setField(editor, "1. d4 d5 2. Nf3 (2. c4 e6) *");
  await action("Save moves");
  await waitFor(async () => (await game(second)).moves === "d4 d5 Nf3" && (await pgn(second)).includes("c4 e6"), "valid PGN edit lost branch");
  const valid = await pgn(second);
  await action("Edit moves");
  await waitFor(() => evaluate(`document.querySelector(${JSON.stringify(editor)})?.value.includes("c4 e6")`), "editor did not reopen saved edit");
  await setField(editor, "1. e5 *");
  await action("Save moves");
  await waitFor(() => evaluate('/invalid|illegal|cannot|failed/i.test(document.querySelector(".topbar .status")?.textContent)'), "invalid SAN did not report an error");
  if (await pgn(second) !== valid) throw new Error("invalid edit changed saved game");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector("main")?.dataset.view === "analysis"'), "edited workspace did not restore");
  await click('[data-analysis-view="detailed"]');
  await node("c4", true);
  if (await pgn(second) !== valid) throw new Error("reopened tree or export differs from saved edit");
  console.log("RBC_ANALYSIS_TREE_EDITS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ANALYSIS_TREE_EDITS_FAIL");
  process.exitCode = 1;
});
