const assert = require("node:assert/strict");
const { mkdirSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");
const { createHash } = require("node:crypto");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, evaluate, click, clickText, destination, setField, view }) => {
  const directory = join(process.env.TMPDIR, "syzygy");
  mkdirSync(directory);
  for (const [type,file] of [["wdl","KQvK.rtbw"],["dtz","KQvK.rtbz"]]) {
    const url = "https://tablebase.lichess.ovh/tables/standard/3-4-5-" + type + "/" + file;
    const response = await fetch(url, {signal:AbortSignal.timeout(15000)});
    assert.ok(response.ok, file + " download failed");
    const bytes = Buffer.from(await response.arrayBuffer());
    writeFileSync(join(directory,file),bytes);
    console.log(JSON.stringify({file,url,sha256:createHash("sha256").update(bytes).digest("hex")}));
  }
  await view("home");
  await destination("settings");
  await view("settings");
  await setField('input[placeholder="/path/to/syzygy"]',directory);
  await clickText(".settings-form button","Save");
  await waitFor(() => evaluate(`fetch("/api/settings").then(r=>r.json()).then(s=>s.syzygy_path===${JSON.stringify(directory)})`), "tablebase path did not save");
  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  const action = text => clickText(".detailed-analysis button",text);
  const load = async position => {
    await setField(".recorded-board .load input",position);
    await click(".recorded-board .load button");
    await waitFor(() => evaluate(`document.querySelector(".recorded-board .fen-line code")?.textContent === ${JSON.stringify(position)}`), "position did not load");
  };
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await original(...args);
      const path = new URL(args[0].url || args[0], location.href).pathname;
      if (path === "/api/cloud-eval" || path === "/api/tablebase") {
        window.lastEvaluation = {path,ok:response.ok,body:await response.clone().json()};
      }
      return response;
    };
  })()`);
  await action("Cloud eval");
  await waitFor(() => evaluate('window.lastEvaluation?.path === "/api/cloud-eval"'), "cloud response missing");
  const cloud = await evaluate("window.lastEvaluation");
  assert.ok(cloud.ok, JSON.stringify(cloud.body));
  assert.ok(cloud.body.pvs?.length, "starting position lacks a cloud evaluation");
  await waitFor(() => evaluate('document.querySelector(".detailed-analysis")?.textContent.includes("Cloud") && document.querySelector(".detailed-analysis")?.textContent.includes("depth")'), "cloud result did not render");
  const position = "7k/8/5KQ1/8/8/8/8/8 w - - 0 1";
  await load(position);
  assert.equal(await evaluate('!![...document.querySelectorAll(".detailed-analysis .best")].find(el=>el.textContent.startsWith("Cloud "))'),false, "cloud result remained after position change");
  await action("Probe");
  await waitFor(() => evaluate('window.lastEvaluation?.path === "/api/tablebase"'), "tablebase response missing");
  const table = await evaluate("window.lastEvaluation");
  assert.ok(table.ok, JSON.stringify(table.body));
  assert.equal(table.body.wdl,"win");
  assert.equal(table.body.pieces,3);
  assert.match(table.body.best_move,/^[a-h][1-8][a-h][1-8]$/);
  await waitFor(() => evaluate(`document.querySelector(".detailed-analysis")?.textContent.includes("3 pieces") && document.querySelector(".detailed-analysis")?.textContent.includes(${JSON.stringify(table.body.best_move)})`), "WDL or best move missing");
  await load("8/8/8/8/8/2k5/5R2/7K w - - 0 1");
  await evaluate("window.lastEvaluation = null");
  await action("Probe");
  await waitFor(() => evaluate('window.lastEvaluation?.path === "/api/tablebase"'), "missing tablebase request did not complete");
  assert.equal(await evaluate("window.lastEvaluation.ok"),false);
  assert.equal(await evaluate('document.querySelector(".detailed-analysis")?.textContent.includes("3 pieces")'),false, "failed probe retained old WDL");

  await load("7k/8/8/8/8/Q7/8/7K b - - 17 39");
  await evaluate("window.lastEvaluation = null");
  await action("Cloud eval");
  await waitFor(() => evaluate('window.lastEvaluation?.path === "/api/cloud-eval"'), "uncached cloud request did not complete");
  assert.equal(await evaluate("window.lastEvaluation.body.available"), false, "uncached fixture unexpectedly has a cloud evaluation");
  await waitFor(() => evaluate('document.querySelector(".detailed-analysis")?.textContent.includes("no cloud evaluation")'), "unavailable cloud result was not shown");

  await clickText(".recorded-board button","Paste PGN…");
  await setField(".recorded-board .pgn-box textarea",'[Variant "Atomic"]\n[Result "*"]\n\n*');
  await clickText(".recorded-board .pgn-box button","Load PGN");
  await waitFor(() => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent.startsWith("rnbqkbnr/")'), "variant PGN did not load");
  await evaluate("window.lastEvaluation = null");
  await action("Probe");
  await waitFor(() => evaluate('window.lastEvaluation?.body?.error?.includes("standard chess")'), "unsupported variant did not report refusal");
  console.log("RBC_CLOUD_TABLEBASE_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_CLOUD_TABLEBASE_FAIL");
  process.exitCode = 1;
});
