// RBCUI-AN-01 and AN-06. Compare the UI, Markdown export and written PGN annotations
// against a real Stockfish classification of a known mating sequence.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, destination, view }) => {
  await view("home");
  const game = await evaluate(`(async () => {
    const pgn = '[White "Report White"]\\n[Black "Report Black"]\\n[Result "0-1"]\\n\\n1. f3 e5 {[%clk 0:02:57]} (1... d5 2. e4 (2. d4 Nf6)) 2. g4 Qh4# 0-1';
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify({pgn}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  if (!game) throw new Error("could not seed report game");
  const configured = await evaluate('fetch("/api/settings", {method:"PUT",headers:{"content-type":"application/json"},body:JSON.stringify({depth:8})}).then(response=>response.ok)');
  if (!configured) throw new Error("could not set report depth");
  await command("Page.reload");
  await view("home");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${game}"]')`), "report fixture not listed");
  await click(`[data-game-id="${game}"]`);
  await view("analysis");
  await click('[data-analysis-view="detailed"]');

  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await original(...args);
      const url = new URL(args[0].url || args[0], location.href);
      if (url.pathname === "/api/games/${game}/report" && response.ok) window.reportFixture = (await response.clone().json()).report;
      return response;
    };
    HTMLAnchorElement.prototype.click = function() {
      window.reportName = this.download;
      if (this.href.startsWith("data:text/markdown")) window.reportText = decodeURIComponent(this.href.split(",").slice(1).join(","));
    };
  })()`);
  await clickText(".detailed-analysis button", "Classify game");
  await waitFor(() => evaluate('window.reportFixture?.moves.length === 4 && document.querySelectorAll(".report .mv-list .mv").length === 4'), "classification did not show all moves");
  const report = await evaluate("window.reportFixture");
  const accuracy = `White ${report.white_accuracy.toFixed(1)}% · Black ${report.black_accuracy.toFixed(1)}%`;
  if (!await evaluate(`document.querySelector(".report .acc")?.textContent === ${JSON.stringify(accuracy)}
    && document.querySelector(".report")?.textContent.includes("Expected points")
    && document.querySelector(".report .spark polyline")?.getAttribute("points").split(" ").length === 4`)) throw new Error("report is missing accuracy, expected points or history");
  const expectedLabels = report.moves.map(move => move.lichess);
  const labels = await evaluate('[...document.querySelectorAll(".report .mv-list .mv")].map(item => item.classList[1])');
  if (JSON.stringify(labels) !== JSON.stringify(expectedLabels)) throw new Error("move labels did not match report");
  await waitFor(() => evaluate('!!document.querySelector(".eval-graph polyline")'), "cached evaluations did not render");

  const fen = () => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent');
  const selected = () => evaluate('document.querySelector(".tree .tnode.current")?.textContent');
  const hasNode = (san, variation) => evaluate(`[...document.querySelectorAll(".tree .tnode" + (${variation} ? ".variation" : ":not(.variation)"))]
      .some(node => node.firstChild.textContent.trim().split(/\\s+/).includes(${JSON.stringify(san)}))`);
  const pick = (san, variation) => evaluate(`[...document.querySelectorAll(".tree .tnode" + (${variation} ? ".variation" : ":not(.variation)"))]
      .find(node => node.firstChild.textContent.trim().split(/\\s+/).includes(${JSON.stringify(san)})).click()`);
  await waitFor(() => hasNode("e5", false), "mainline node missing");
  await pick("e5", false);
  await waitFor(async () => (await selected())?.includes("e5") && (await fen())?.includes("/4p3/"), "mainline replay missing");
  const mainlineFen = await fen();
  if (!await evaluate('document.querySelector(".tree .tnode.current")?.textContent.includes("0:02:57")')) throw new Error("clock comment missing from selected mainline move");

  await waitFor(() => hasNode("Nf6", true), "nested variation node missing");
  await pick("Nf6", true);
  await waitFor(async () => (await selected())?.includes("Nf6") && (await fen())?.includes("/5n2/"), "nested variation did not update the board");
  const nestedFen = await fen();
  await click('[data-analysis-view="simple"]');
  await click('[data-analysis-view="detailed"]');
  if (await fen() !== nestedFen || !(await selected())?.includes("Nf6")) throw new Error("mode switch lost the nested variation");

  await evaluate(`(() => {
    const svg = document.querySelector(".eval-svg");
    const bounds = svg.getBoundingClientRect();
    const row = svg.querySelector("polyline").getAttribute("points").trim().split(" ")[1];
    const x = Number(row.split(",")[0]);
    svg.dispatchEvent(new MouseEvent("mousemove", {bubbles:true,clientX:bounds.left + x * bounds.width / 100,clientY:bounds.top + bounds.height / 2}));
  })()`);
  await waitFor(() => evaluate('document.querySelector(".eval-readout")?.textContent.includes("ply 1 e5")'), "cached graph hover did not identify the mainline move");
  await evaluate('document.querySelector(".eval-svg").dispatchEvent(new MouseEvent("click", {bubbles:true}))');
  await waitFor(async () => await fen() === mainlineFen && (await selected())?.includes("e5"), "cached graph did not jump back to the recorded mainline");

  await clickText(".detailed-analysis button", "Export report");
  await waitFor(() => evaluate('window.reportName?.endsWith("-report.md") && !!window.reportText'), "Markdown export missing");
  const markdown = await evaluate("window.reportText");
  if (!markdown.includes(accuracy) || !markdown.includes("| Ply | Move | Label | Accuracy |")) throw new Error("exported accuracy disagrees with displayed accuracy");

  const marked = report.moves.filter(move => ["brilliant","great","inaccuracy","mistake","blunder"].includes(move.lichess));
  if (!marked.length) throw new Error("fixture produced no annotatable moves");
  await clickText(".detailed-analysis button", "Annotate game");
  await waitFor(() => evaluate(`fetch("/api/games/${game}/annotations").then(response => response.json()).then(rows => rows.length >= ${marked.length})`), "annotations were not written");
  const annotations = await evaluate(`fetch("/api/games/${game}/annotations").then(response => response.json())`);
  if (!marked.every(move => annotations.some(row => row.ply === move.ply && row.nag > 0 && row.comment.includes(`${move.accuracy.toFixed(0)}% accuracy`)))) {
    throw new Error("written NAGs or accuracy comments do not match report");
  }
  await command("Page.reload");
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  await waitFor(() => evaluate('document.querySelector(".tree")?.textContent.includes("•")'), "recorded annotations did not restore");
  console.log("RBC_GAME_REPORT_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_GAME_REPORT_FAIL");
  process.exitCode = 1;
});
