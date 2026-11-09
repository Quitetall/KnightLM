// RBCUI-SHELL-05. Feed a session in the pinned baseline's v1 wire format.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, clickText, view }) => {
  await view("home");
  await waitFor(() => evaluate('document.querySelector(".session-notice")?.textContent.includes("older session")'), "migration notice did not show");
  await click(".session-notice button");
  await waitFor(() => evaluate('!document.querySelector(".session-notice")'), "migration notice did not dismiss");

  const fixture = await evaluate(`(async () => {
    const post = (route, payload, method = "POST") => fetch(route, {
      method, headers:{"content-type":"application/json"}, body:JSON.stringify(payload),
    }).then(async response => {
      if (!response.ok) throw new Error(route + " " + response.status);
      return response.json();
    });
    const pgn = '[White "Saved White"]\\n[Black "Saved Black"]\\n[Result "*"]\\n\\n1. e4 e5 2. Nf3 *';
    const game_id = (await post("/api/games",{pgn})).imported[0];
    const workspace = await post("/api/workspaces",{title:"Baseline analysis",kind:"analysis"});
    const start_fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const createBoard = (title, moves) => post("/api/boards", {
      parent_kind:"workspace",parent_id:workspace.id,kind:"analysis",title,start_fen,content_json:JSON.stringify({moves}),
    });
    const first = await createBoard("Baseline board A",["d4","d5"]);
    const chosen = await createBoard("Baseline board B",["e4","e5","Nf3"]);
    const empty = (key,kind,title) => ({key,kind,title,game_id:null,pgn:"",ply:0,variant:"standard",preview_only:false,moves:[]});
    const workspaces = [
      empty(41,"database","Saved games"),
      {...empty(42,"analysis","Saved analysis"), game_id,pgn,ply:2,moves:["e4","e5","Nf3"],boards:[first,chosen],active_board:1,server_id:workspace.id},
      empty(43,"openings","Saved openings"),
      empty(44,"puzzles","Saved puzzles"),
    ];
    const client = "paritybaseline42";
    const payload = JSON.stringify({workspaces,active:42,seq:45,tab:"analysis"});
    await post("/api/session",{client,payload},"PUT");
    document.cookie = "rbc_client=" + client + "; Path=/; SameSite=Lax";
    localStorage.removeItem("rbc.session.v1");
    localStorage.removeItem("rbc.client");
    return {client, game_id, workspace:workspace.id, chosen:chosen.id};
  })()`);
  await command("Page.navigate", {url:base + "?legacy-cold"});
  await view("home");
  const saved = () => evaluate('JSON.parse(localStorage.getItem("rbc.session.v1") || "null")');
  await waitFor(async () => (await saved())?.workspaces?.length === 4, "cold launch did not restore baseline session");
  const checkSnapshot = async expectedTab => {
    const value = await saved();
    if (value?.tab !== expectedTab || value.active !== 42 ||
      value.workspaces.map(item => item.key + ":" + item.kind + ":" + item.title).join(",")
        !== "41:database:Saved games,42:analysis:Saved analysis,43:openings:Saved openings,44:puzzles:Saved puzzles") return false;
    const game = value.workspaces[1];
    return game.game_id === fixture.game_id && game.ply === 2 && game.server_id === fixture.workspace
      && game.boards[game.active_board]?.id === fixture.chosen
      && JSON.parse(game.boards[game.active_board].content_json).moves.join(" ") === "e4 e5 Nf3";
  };
  await waitFor(() => checkSnapshot("home"), "cold launch lost game, board, order, active workspace, or selected ply");
  await clickText(".ws-name", "Saved analysis");
  await view("analysis");
  const expectedFen = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
  const visible = () => evaluate(`document.querySelector(".recorded-board .fen-line code")?.textContent === "${expectedFen}"
    && document.querySelector(".board-chip.active")?.textContent.includes("Baseline board B")`);
  await waitFor(visible, "selected board or visible ply did not restore");
  await waitFor(() => checkSnapshot("analysis"), "analysis snapshot did not persist");
  await waitFor(() => evaluate(`fetch("/api/session?client=${fixture.client}").then(response => response.json()).then(data => JSON.parse(data.payload).tab === "analysis")`), "server did not receive saved session");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector("main")?.dataset.view === "analysis"'), "reload did not resume Analysis");
  await waitFor(visible, "ordinary reload lost selected board");
  if (await evaluate('!!document.querySelector(".session-notice")')) throw new Error("dismissed notice returned on reload");
  await evaluate("localStorage.clear()");
  await command("Page.reload");
  await waitFor(async () => await evaluate(`localStorage.getItem("rbc.client") === "${fixture.client}"`) && await checkSnapshot("analysis") && await visible(), "cookie/server fallback did not restore exact state");
  console.log("RBC_WORKSPACE_RESTORE_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_WORKSPACE_RESTORE_FAIL");
  process.exitCode = 1;
});
