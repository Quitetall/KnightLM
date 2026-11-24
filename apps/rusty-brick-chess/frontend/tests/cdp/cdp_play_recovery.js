// RBCUI-PLAY-03. Drive the actual board and recover from a failed model reply.
const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, setField, destination, view }) => {
  const api = (path, method = "GET", data) => fetch(new URL("/api/" + path, base), {
    method, headers: {"content-type":"application/json"}, body: JSON.stringify(data),
  });
  const json = async (...args) => {
    const response = await api(...args);
    assert.ok(response.ok, args[0] + " returned " + response.status);
    return response.json();
  };
  const text = selector => evaluate(`document.querySelector(${JSON.stringify(selector)})?.textContent.trim()`);
  const status = () => text(".match-status");
  const selected = () => evaluate('document.querySelector(".match-card.active")?.dataset.matchId');
  const moves = game => game.moves.map(move => move.san).join(" ");
  const boardClick = async square => {
    const point = await evaluate(`(() => {
      const r = document.querySelector('.active-match [data-square="${square}"]').getBoundingClientRect();
      return {x:r.left+r.width/2,y:r.top+r.height/2};
    })()`);
    for (const type of ["mousePressed","mouseReleased"]) {
      await command("Input.dispatchMouseEvent", {type,...point,button:"left",clickCount:1});
    }
  };
  const play = async (from, to) => {
    await boardClick(from);
    await boardClick(to);
  };

  await view("home");
  await destination("play");
  await view("play");
  await waitFor(() => evaluate('!!document.querySelector(".play-opponent option[value=\\"0\\"]")'), "default model missing");
  await setField(".play-opponent", "0", "change");
  await click(".play-start");
  await waitFor(async () => (await status()) === "Your move" && await selected(), "default model game did not open");
  const id = await selected();
  const path = "ai/matches/" + id;
  const game = () => json(path);
  const initial = await game();
  await play("e2", "e5");
  assert.equal((await game()).pgn, initial.pgn, "illegal board input modified the game");

  // Fail at the actual server/provider boundary. Hold that response in the
  // browser until the thinking state and blocked board input are observed.
  const modelUrl = (await json("settings")).model_url;
  await json("settings", "PUT", {model_url: new URL("/no-provider-here", base).href});
  await evaluate(`(() => {
    const fetchOriginal = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await fetchOriginal(...args);
      if (new URL(args[0].url || args[0], location.href).pathname.endsWith("/model-move") && !response.ok) {
        await new Promise(resolve => { window.releaseFailure = resolve; });
      }
      return response;
    };
  })()`);
  await play("e2", "e4");
  await waitFor(async () => (await status())?.includes("thinking") && await evaluate("!!window.releaseFailure"), "thinking state did not appear");
  await play("e7", "e5");
  assert.equal(moves(await game()), "e4", "board accepted a move while model was busy");
  assert.equal((await api(path + "/move", "POST", {mv:"e2e4"})).status, 400, "duplicate server action was accepted");
  await evaluate("window.releaseFailure()");
  await waitFor(async () => !!(await text(".match-error")) && (await status()) === "Opponent to move", "provider failure was not visible");
  await json("settings", "PUT", {model_url: modelUrl});
  await click(".match-model-move");
  await waitFor(async () => moves(await game()) === "e4 e5" && (await status()) === "Your move", "manual retry failed");
  assert.ok((await game()).pgn.includes("1. e4 e5 *"));

  await play("g1", "f3");
  await waitFor(async () => (await game()).moves.length === 4 && (await status()) === "Your move", "auto reply did not finish");
  assert.match(await text(".match-moves"), /Nf3/);
  await click(".match-takeback");
  await waitFor(async () => (await game()).moves.length === 3 && (await status()) === "Opponent to move", "takeback did not remove the reply");
  assert.ok((await game()).pgn.includes("2. Nf3 *"));
  await click(".match-resign");
  await waitFor(async () => (await game()).result?.score === "0-1" && (await status())?.includes("resignation"), "resignation did not finish the game");
  const ended = await game();
  assert.ok(ended.pgn.includes('[Result "0-1"]'));
  await click(".match-rematch");
  await waitFor(async () => !!await selected() && (await selected()) !== id && (await status()) === "Your move", "rematch did not open a new game");
  assert.equal((await game()).pgn, ended.pgn, "rematch altered original");
  assert.deepEqual((await json("ai/matches/" + await selected())).moves, []);
  await evaluate('document.querySelector(".play-more").open = true');
  await click(`[data-match-id="${id}"]`);
  await waitFor(async () => (await status())?.includes("resignation"), "could not select the finished game");
  await click(".match-reset");
  await waitFor(async () => (await status()) === "Your move" && (await game()).pgn === initial.pgn, "reset did not restore original position");
  const row = (await json("ai/matches")).find(match => match.id === id);
  assert.equal(row.ended, null, "reset left the game marked ended in Games");
  console.log("RBC_PLAY_RECOVERY_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_PLAY_RECOVERY_FAIL");
  process.exitCode = 1;
});
