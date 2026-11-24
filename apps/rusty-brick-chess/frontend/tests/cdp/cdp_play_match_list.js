// RBCUI-PLAY-04. Read timed/live and completed games through their match cards.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, destination, view }) => {
  await view("home");
  const fixtures = await evaluate(`(async () => {
    const post = (route, body) => fetch(route,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(body)})
      .then(async response => {
        if (!response.ok) throw new Error(route + " " + await response.text());
        return response.json();
      });
    const start_fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const white = {kind:"model",provider:"mock",model:"TimedWhite"};
    const black = {kind:"human",name:"Timed Black"};
    const live = await post("/api/ai/matches",{config:{start_fen,white,black,max_plies:100,time_control:{initial_ms:180000,increment_ms:2000}}});
    await post("/api/ai/matches/" + live.id + "/model-move",{});
    const finished = await post("/api/ai/matches",{config:{
      start_fen,white:{kind:"model",provider:"mock",model:"DrawWhite"},
      black:{kind:"model",provider:"mock",model:"DrawBlack"},max_plies:1,
    }});
    await post("/api/ai/matches/" + finished.id + "/watch/step",{plies:1});
    const detail = id => fetch("/api/ai/matches/" + id).then(response=>response.json());
    return {live:await detail(live.id),finished:await detail(finished.id)};
  })()`);
  const showGames = async () => {
    await destination("play");
    await view("play");
    await evaluate('document.querySelector(".play-more").open = true');
  };
  await showGames();
  const card = id => `[data-match-id="${id}"]`;
  const text = selector => evaluate(`document.querySelector(${JSON.stringify(selector)})?.textContent.trim()`);
  await waitFor(async () => (await text(card(fixtures.live.id)))?.includes("live") && (await text(card(fixtures.finished.id)))?.includes("ended"), "live or ended match list state missing");
  await click(card(fixtures.live.id));
  const clock = ms => {
    const total = Math.floor(ms / 1000);
    return [Math.floor(total / 60), total % 60].map(n => String(n).padStart(2,"0")).join(":");
  };
  await waitFor(async () => (await text(".match-cards"))?.includes([fixtures.live.clock.white_ms, fixtures.live.clock.black_ms].map(clock).join(" / "))
    && (await text(".match-status")) === "Your move", "live clocks or turn did not match payload");
  if (!(await text(".match-cards"))?.includes("TimedWhite vs Timed Black")) throw new Error("seat names missing");
  if (await evaluate('document.querySelector(".active-match .board [data-square]")?.dataset.square') !== "h1") throw new Error("Black perspective missing");
  if (!(await text(".match-moves"))?.includes(fixtures.live.moves[0].san)) throw new Error("live SAN missing");
  await click(card(fixtures.finished.id));
  await waitFor(async () => (await text(".match-status"))?.includes("Draw") && (await text(".match-status"))?.includes("max plies"), "draw result or termination not shown");
  if (!await evaluate('document.querySelector(".match-step")?.disabled && document.querySelector(".match-step-5")?.disabled')) throw new Error("completed watch controls remained enabled");
  if (!(await text(".match-cards"))?.includes("DrawWhite vs mock · DrawBlack")) throw new Error("completed seats missing");

  await click(card(fixtures.live.id));
  await waitFor(async () => (await text(".match-status")) === "Your move", "live game did not switch back");
  await destination("home");
  await view("home");
  await showGames();
  await waitFor(() => evaluate(`document.querySelector(".match-card.active")?.dataset.matchId === "${fixtures.live.id}"`), "return to Play did not reopen the same match");
  if (!(await text(".match-moves"))?.includes(fixtures.live.moves[0].san)) throw new Error("resumed match lost its notation");
  console.log("RBC_PLAY_MATCH_LIST_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_PLAY_MATCH_LIST_FAIL");
  process.exitCode = 1;
});
