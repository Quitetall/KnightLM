// RBCUI-BOARD-01. Real pointer and touch input on the shared board.
const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  const board = ".recorded-board";
  const fen = () => evaluate(`document.querySelector("${board} .fen-line code")?.textContent`);
  const square = name => `${board} .board [data-square="${name}"]`;
  const point = name => evaluate(`(() => {
    const cell = document.querySelector(${JSON.stringify(square(name))});
    const rect = cell.getBoundingClientRect();
    return {x:rect.left + rect.width/2, y:rect.top + rect.height/2};
  })()`);
  const mouse = async (type, xy) => command("Input.dispatchMouseEvent", {
    type, ...xy, button: type === "mouseMoved" ? "none" : "left",
    buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1,
  });
  const tap = async name => {
    const xy = await point(name);
    await mouse("mousePressed", xy);
    await mouse("mouseReleased", xy);
  };
  const drag = async (from, to) => {
    const a = await point(from), b = await point(to);
    await mouse("mousePressed", a);
    await mouse("mouseMoved", {x:(a.x+b.x)/2,y:(a.y+b.y)/2});
    await mouse("mouseMoved", b);
    await mouse("mouseReleased", b);
  };
  const has = (name, value) => evaluate(`document.querySelector(${JSON.stringify(square(name))})?.classList.contains(${JSON.stringify(value)})`);
  const loadFen = async value => {
    await setField(board + " .load input", value);
    await click(board + " .load button");
    await waitFor(async () => (await fen()) === value, "FEN did not load");
  };
  const loadVariant = async (variant, position) => {
    await clickText(board + " button", "Paste PGN…");
    await setField(board + " .pgn-box textarea", `[Variant "${variant}"]\n[SetUp "1"]\n[FEN "${position}"]\n[Result "*"]\n\n*`);
    await clickText(board + " .pgn-box button", "Load PGN");
    await waitFor(async () => (await fen())?.startsWith(position.split(" ")[0]), variant + " did not load");
  };

  const start = await fen();
  await drag("e2", "e5");
  await waitFor(() => evaluate(`!document.querySelector("${board} .dragging-src")`), "illegal drag did not snap back");
  assert.equal(await fen(), start);
  await tap("e2");
  await waitFor(() => has("e4", "target"), "legal destination not highlighted");
  await tap("e4");
  await waitFor(() => has("e4","dest"), "click move did not land");
  await drag("d7","d5");
  await waitFor(() => has("d5","dest"), "drag move did not land");
  await tap("e4");
  await waitFor(() => has("d5","capture"), "capture target not marked");
  const touch = async name => {
    await command("Input.dispatchTouchEvent", {type:"touchStart",touchPoints:[await point(name)]});
    await command("Input.dispatchTouchEvent", {type:"touchEnd",touchPoints:[]});
  };
  await touch("d5");
  await waitFor(async () => await has("d5","dest") && await has("e4","last"), "touch capture or last-move highlight missing");
  assert.ok((await fen()).includes("/3P4/"));
  assert.equal(await evaluate(`document.querySelectorAll("${board} .captured .cap-row:nth-child(2) .cap").length`), 1);
  await clickText(board + " .controls button", "Flip");
  await waitFor(() => evaluate(`document.querySelector("${board} .board [data-square]")?.dataset.square === "h1"`), "flip did not reverse board");

  await loadFen("7k/P7/8/8/8/8/8/7K w - - 0 1");
  await tap("a7");
  await tap("a8");
  await waitFor(() => evaluate('document.querySelectorAll(".promo-btn").length === 4'), "promotion choices missing");
  await click(".promo-btn:nth-child(4)");
  await waitFor(async () => (await fen())?.startsWith("N6k/"), "underpromotion did not apply");

  await loadVariant("Crazyhouse", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/R1BQKBNR[N] w KQkq - 0 1");
  await click('.pocket-piece[title="Drop N"]');
  await tap("f3");
  await waitFor(async () => (await fen())?.includes("/5N2/"), "pocket drop did not apply");
  assert.equal(await evaluate('!!document.querySelector(\'.pocket-piece[title="Drop N"]\')'), false);

  await loadVariant("Chess960", "4k3/8/8/8/8/8/8/1K3R2 w F - 0 1");
  await tap("b1");
  await drag("b1","f1");
  await waitFor(async () => (await fen())?.split(" ")[0].endsWith("/5RK1"), "Chess960 castling did not move king to g1");
  console.log("RBC_BOARD_GESTURES_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_BOARD_GESTURES_FAIL");
  process.exitCode = 1;
});
