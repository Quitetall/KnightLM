// RBCUI-TRAIN-03. Create and import from the Library, then review from Train.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, destination, setField, view }) => {
  const list = () => evaluate('fetch("/api/repertoires").then(r => r.json())');
  const due = id => evaluate(`fetch("/api/repertoires/${id}/next").then(r => r.json())`);
  await view("home");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await clickText("main button", "Repertoires");
  await view("openings");

  const create = async (name, color) => {
    await setField('.opening-shell input[placeholder="Repertoire name…"]', name);
    await setField('.opening-shell select:has(option[value="white"])', color, "change");
    await clickText(".opening-shell .dbs button", "Create");
    await waitFor(async () => (await list()).some(rep => rep.name === name && rep.color === color), "repertoire not created");
  };
  await create("White fixture", "white");
  await create("Black fixture", "black");
  await waitFor(() => evaluate('document.querySelectorAll(".rep-row").length === 2'), "repertoire rows missing");
  const white = (await list()).find(rep => rep.color === "white");
  const black = (await list()).find(rep => rep.color === "black");
  const row = color => `.rep-row:nth-child(${color === "black" ? 1 : 2})`;
  await clickText(row("white") + " button", "Import PGN");
  await setField(".opening-shell .pgn-box textarea", '[Result "*"]\n\n1. e4 (1. d4 d5 2. c4) e5 2. Nf3 *');
  await clickText(".opening-shell .pgn-box button", "Import into selected repertoire");
  await waitFor(async () => (await list()).some(rep => rep.id === white.id && rep.cards === 4 && rep.due === 4), "branch did not import four White cards");

  await destination("train");
  await click('[data-open-view="openings"]');
  await view("openings");
  await clickText(".opening-shell button", "Repertoires");
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await original(...args);
      const url = new URL(args[0].url || args[0], location.href);
      if (url.pathname === "/api/repertoires/review" && response.ok) window.repertoireReview = await response.clone().json();
      return response;
    };
  })()`);
  const board = ".opening-shell .analysis .board";
  const play = async uci => {
    for (const square of [uci.slice(0,2),uci.slice(2,4)]) {
      const point = await evaluate(`(() => {
        const target = document.querySelector('${board} [data-square="${square}"]');
        if (!target) return null;
        target.scrollIntoView({block:"nearest"});
        const rect = target.getBoundingClientRect();
        return {x:rect.x + rect.width / 2, y:rect.y + rect.height / 2};
      })()`);
      if (!point) throw new Error("training board square missing");
      for (const type of ["mousePressed","mouseReleased"]) {
        await command("Input.dispatchMouseEvent", {type,...point,button:"left",clickCount:1});
      }
    }
  };
  await clickText(row("white") + " button", "Train");
  const first = await due(white.id);
  const fen = () => evaluate('document.querySelector(".opening-shell .fen-line code")?.textContent');
  const position = value => value?.split(" ").slice(0,4).join(" ");
  const expectCard = async card => {
    try {
      await waitFor(async () => position(await fen()) === position(card.fen), "due card did not load");
    } catch (error) {
      console.log(JSON.stringify({card, rendered:await fen(), rows:await evaluate('document.querySelector(".rep-list")?.textContent')}));
      throw error;
    }
  };
  await expectCard(first);
  await play(first.uci);
  await waitFor(() => evaluate(`window.repertoireReview?.id === ${first.id} && document.querySelector(".opening-shell .engine-pane")?.textContent.includes("Correct")`), "correct move not recorded");
  const success = await evaluate("window.repertoireReview");
  if (success.reviews !== 1 || success.lapses !== 0 || success.interval_days !== 1 || success.ease <= first.ease || success.due_at <= first.due_at) throw new Error("correct review scheduling mismatch");
  await clickText(".opening-shell .engine-pane button", "Next card");
  const second = await due(white.id);
  if (!second || first.id === second.id) throw new Error("Next card did not advance");
  await expectCard(second);
  await play("a2a3");
  await waitFor(() => evaluate(`window.repertoireReview?.id === ${second.id} && document.querySelector(".opening-shell .engine-pane")?.textContent.includes("Wrong")`), "wrong move not recorded");
  const failure = await evaluate("window.repertoireReview");
  if (failure.reviews !== 1 || failure.lapses !== 1 || failure.interval_days !== 1 || failure.ease >= second.ease || failure.due_at <= second.due_at) throw new Error("wrong review scheduling mismatch");
  await command("Page.reload");
  await view("openings");
  await clickText(".opening-shell button", "Repertoires");
  await waitFor(() => evaluate('document.querySelector(".rep-list")?.textContent.includes("4 cards · 2 due")'), "due count did not survive reload");
  await clickText(row("black") + " button", "Delete");
  await waitFor(async () => (await list()).length === 1 && (await list())[0].id === white.id, "delete affected the wrong repertoire");
  if (await due(black.id)) throw new Error("deleted repertoire still has due cards");
  console.log("RBC_REPERTOIRE_TRAINING_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_REPERTOIRE_TRAINING_FAIL");
  process.exitCode = 1;
});
