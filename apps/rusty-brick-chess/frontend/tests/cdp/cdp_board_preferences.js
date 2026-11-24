// RBCUI-CFG-05 and RBCUI-BOARD-03 on the shared Analysis board.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, clickText, setField, destination, view }) => {
  const analysis = async () => {
    await destination("workbench");
    await click('[data-open-view="analysis"]');
    await view("analysis");
    await click('[data-analysis-view="detailed"]');
  };
  await view("home");
  await destination("settings");
  await view("settings");
  const lang = '.settings-form select:has(option[value="es"])';
  await setField(lang, "es", "change");
  await waitFor(() => evaluate('document.querySelector("[data-destination=train]")?.textContent.trim() === "Entrenar"'), "Spanish did not apply");
  await waitFor(() => evaluate('fetch("/api/settings").then(response => response.json()).then(data => data.language === "es")'), "Spanish did not persist");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector("[data-destination=train]")?.textContent.trim() === "Entrenar"'), "Spanish did not restore");
  await view("settings");
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.parityPuts = 0;
    window.fetch = async (...args) => {
      const request = args[0];
      if (new URL(request.url || request, location.href).pathname === "/api/settings"
          && (request.method || args[1]?.method) === "PUT") {
        window.parityPuts += 1;
        if (window.parityPuts === 1) await new Promise(resolve => { window.releaseFirstSettings = resolve; });
      }
      return original(...args);
    };
  })()`);
  await setField(lang, "en", "change");
  await waitFor(() => evaluate('document.querySelector("[data-destination=train]")?.textContent.trim() === "Train"'), "English did not apply");
  const bindings = ["a", "s", "d", "g", "z"];
  for (const [index, binding] of bindings.entries()) {
    await setField(`.keybinds label:nth-child(${index + 1}) input`, binding, "change");
  }
  const prefsButton = '.keybinds + div + .row-btns button';
  await clickText(prefsButton, "Save");
  await waitFor(() => evaluate('!!window.releaseFirstSettings'), "first settings request was not intercepted");
  if (await evaluate("window.parityPuts") !== 1) throw new Error("newer settings were sent while the old snapshot was in flight");
  await evaluate("window.releaseFirstSettings()");
  const stored = () => evaluate('fetch("/api/settings").then(response => response.json()).then(data => JSON.parse(data.keybinds))');
  await waitFor(async () => {
    const keys = await stored();
    return keys && ["previous","next","start","end","flip"].map(key => keys[key]).join(",") === bindings.join(",");
  }, "keybindings did not persist");

  await analysis();
  const board = ".recorded-board";
  const fen = () => evaluate(`document.querySelector("${board} .fen-line code")?.textContent`);
  const firstSquare = () => evaluate(`document.querySelector("${board} .board [data-square]")?.dataset.square`);
  const startFen = await fen();
  const loadPgn = async text => {
    await clickText(board + " button", "Paste PGN…");
    await setField(board + " .pgn-box textarea", text);
    await click(board + " .pgn-box button");
  };
  await loadPgn("[Result \"*\"]\n\n1. e4 e5 2. Nf3 *");
  await waitFor(async () => !!await fen() && (await fen()) !== startFen, "PGN did not change board");
  const endFen = await fen();
  const key = name => evaluate(`document.querySelector("${board} .board-wrap").dispatchEvent(new KeyboardEvent("keydown",{key:${JSON.stringify(name)},bubbles:true}))`);
  await key("d");
  await waitFor(async () => await fen() === startFen, "Start key missed");
  await key("s");
  await waitFor(async () => (await fen())?.includes(" b "), "Next key missed");
  await key("a");
  await waitFor(async () => await fen() === startFen, "Previous key missed");
  await key("g");
  await waitFor(async () => await fen() === endFen, "End key missed");
  const first = await firstSquare();
  await key("z");
  await waitFor(async () => !!await firstSquare() && (await firstSquare()) !== first, "Flip key missed");

  await click(board + ' button[title="Previous"]');
  await waitFor(async () => await fen() !== endFen, "previous button did not move");
  await click(board + ' button[title="Next"]');
  await waitFor(async () => await fen() === endFen, "next button did not move");
  await clickText(board + " .movelist button", "e4");
  await waitFor(async () => (await fen())?.includes(" b ") && (await fen())?.includes("/4P3/"), "move-list click did not select e4");
  await waitFor(() => evaluate(`[...document.querySelectorAll("${board} .board [data-square].last")].map(square => square.dataset.square).sort().join(",") === "e2,e4"`), "recorded move highlight did not follow e4");
  const selectedMove = await fen();
  await evaluate(`document.querySelector("${board} .load input").focus()`);
  await command("Input.dispatchKeyEvent", {type:"keyDown", key:"g", text:"g"});
  await command("Input.dispatchKeyEvent", {type:"keyUp", key:"g"});
  if (await fen() !== selectedMove) throw new Error("typing in FEN input moved the board");
  const width = () => evaluate(`document.querySelector("${board} .board-wrap").getBoundingClientRect().width`);
  const originalWidth = await width();
  await setField(board + ' input[type="range"]', "45");
  await waitFor(async () => (await width()) < originalWidth, "board size did not apply");
  await setField(board + " .selects label:first-child select", "blue", "change");
  await waitFor(() => evaluate(`!!document.querySelector("${board} .board.blue")`), "per-board theme did not apply");
  const pieceSelect = board + " .selects label:nth-child(2) select";
  const artwork = () => evaluate(`document.querySelector('${board} [data-square="a1"]').innerHTML`);
  const originalArt = await artwork();
  const nextPiece = await evaluate(`[...document.querySelector(${JSON.stringify(pieceSelect)}).options].find(option => !option.selected).value`);
  await setField(pieceSelect, nextPiece, "change");
  await waitFor(async () => await artwork() !== originalArt, "piece set did not apply");
  await clickText(board + " .controls button", "Reset");
  await waitFor(async () => await fen() === startFen, "Reset did not return to start");
  await loadPgn("[Result \"*\"]\n\n1. e4 e5 2. Nf3 *");
  await waitFor(async () => await fen() === endFen, "board did not reload saved move list");

  await clickText(board + " button", "Paste PGN…");
  await clickText(board + " button", "Hide PGN");
  if (await evaluate(`!!document.querySelector("${board} .pgn-box")`)) throw new Error("PGN editor did not hide");
  await loadPgn("[Result \"*\"]\n\n1. e5 *");
  if (await fen() !== endFen) throw new Error("invalid PGN replaced the board");
  await waitFor(() => evaluate(`/invalid|illegal|error|cannot/i.test(document.querySelector("${board} aside .status")?.textContent)`), "invalid PGN did not report an error");

  const legalFen = "8/8/8/8/8/2k5/5R2/7K w - - 0 1";
  await setField(board + " .load input", legalFen);
  await click(board + " .load button");
  await waitFor(async () => await fen() === legalFen, "valid FEN was not loaded");
  await setField(board + " .load input", "not a FEN");
  await click(board + " .load button");
  if (await fen() !== legalFen) throw new Error("invalid FEN replaced the board");
  await waitFor(() => evaluate(`/fen|invalid|error/i.test(document.querySelector("${board} aside .status")?.textContent)`), "invalid FEN did not report an error");
  await command("Browser.grantPermissions", {origin:new URL(base).origin, permissions:["clipboardReadWrite", "clipboardSanitizedWrite"]});
  await clickText(board + " .fen-line button", "Copy");
  await waitFor(async () => await evaluate("navigator.clipboard.readText()") === legalFen, "clipboard does not contain the board FEN");
  await loadPgn('[Variant "Crazyhouse"]\n[SetUp "1"]\n[FEN "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/R1BQKBNR[N] w KQkq - 0 1"]\n[Result "*"]\n\n1. N@f3 *');
  await waitFor(async () => (await fen())?.includes("/5N2/"), "variant drop was not applied");

  await destination("settings");
  await view("settings");
  await clickText(prefsButton, "Reset keybinds");
  await clickText(prefsButton, "Save");
  await waitFor(async () => (await stored())?.previous === "ArrowLeft" && (await stored())?.flip === "f", "default bindings did not persist");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector(".keybinds input")?.value === "ArrowLeft"'), "default bindings did not restore");
  console.log("RBC_BOARD_PREFERENCES_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_BOARD_PREFERENCES_FAIL");
  process.exitCode = 1;
});
