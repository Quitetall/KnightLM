const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const response = await fetch(new URL("/api/games",base),{method:"POST",headers:{"content-type":"application/json"},
    body:JSON.stringify({pgn:'[Result "1-0"]\n\n1. e4 e5 1-0\n\n[Result "0-1"]\n\n1. d4 d5 0-1'})});
  assert.ok(response.ok);
  await destination("train");
  await click('[data-open-view="openings"]');
  await view("openings");
  const source = '.opening-shell select:has(option[value="masters"])';
  const input = '.opening-shell input[placeholder="FEN…"]';
  const start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
  if (!process.env.RBC_DB) throw new Error("Use the isolated browser harness");
  execFileSync("python3",["-c",`import json,os,sqlite3,sys
fen=sys.argv[1]
with sqlite3.connect(os.environ["RBC_DB"]) as db:
 for source,white,draws,black in [("lichess",31,11,12),("masters",15,20,13)]:
  body=json.dumps({"opening":{"eco":"A00","name":"Fixture "+source},"moves":[{"uci":"e2e4","san":"e4","white":white,"draws":draws,"black":black}]})
  db.execute("INSERT OR REPLACE INTO explorer_cache (source,fen,body) VALUES (?,?,?)",(source,fen,body))
`,start]);

  await evaluate(`(() => {
    const original=window.fetch.bind(window);
    window.fetch=async (...args)=>{
      const response=await original(...args);
      const url=new URL(args[0].url || args[0],location.href);
      if (url.pathname==="/api/openings") window.lastOpening={url:url.href,ok:response.ok,body:await response.clone().json()};
      return response;
    };
  })()`);
  const explore = async kind => {
    await setField(source,kind,"change");
    await evaluate("window.lastOpening = null");
    await clickText(".opening-shell .load button","Explore");
    await waitFor(()=>evaluate("!!window.lastOpening"),"explorer did not return");
    const returned = await evaluate("window.lastOpening");
    assert.ok(returned.ok,JSON.stringify(returned.body));
    assert.equal(new URL(returned.url).searchParams.get("source"),kind==="local"?null:kind);
    const expected=returned.body.continuations.map(move=>[move.san,move.games,move.white,move.draw,move.black].map(String));
    await waitFor(()=>evaluate(`JSON.stringify([...document.querySelectorAll(".opening .row:not(.head)")].map(row=>[...row.children].map(cell=>cell.textContent))) === ${JSON.stringify(JSON.stringify(expected))}`),"continuation counts did not match " + kind);
    if (returned.body.eco) assert.ok(await evaluate(`document.querySelector(".opening-name")?.textContent.includes(${JSON.stringify(returned.body.eco)})`));
    return returned.body;
  };
  await setField(input,start);
  const local = await explore("local");
  assert.equal(local.continuations.reduce((sum,move)=>sum+move.games,0),2);
  for (const kind of ["lichess","masters"]) {
    const remote=await explore(kind);
    assert.ok(remote.continuations.reduce((sum,move)=>sum+move.games,0)>2,kind+" did not use its cached source fixture");
  }
  await setField(source,"local","change");
  await setField(input,"not a FEN");
  await evaluate("window.lastOpening = null");
  await clickText(".opening-shell .load button","Explore");
  await waitFor(()=>evaluate('window.lastOpening?.ok === false'),"invalid FEN did not report failure");
  assert.equal(await evaluate('document.querySelectorAll(".opening .row:not(.head)").length'),0,"failed explorer retained stale continuations");
  await setField(input,"8/8/8/8/8/2k5/5R2/7K w - - 0 1");
  assert.deepEqual((await explore("local")).continuations,[]);
  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  await clickText(".detailed-analysis button","Explorer");
  await view("openings");
  await clickText(".opening-shell .load button","Current board");
  await waitFor(()=>evaluate(`document.querySelector(${JSON.stringify(input)}).value === ${JSON.stringify(start)}`),"Analysis board did not transfer to Explorer");
  await waitFor(()=>evaluate('document.querySelectorAll(".opening .row:not(.head)").length === 2'),"current-board explorer did not query local games");
  console.log("RBC_OPENING_SOURCES_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_OPENING_SOURCES_FAIL");
  process.exitCode=1;
});
