const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, evaluate, click, clickText, destination, view }) => {
  if (!process.env.RBC_DB) throw new Error("Use the isolated browser harness");
  const api = async (path, body, method = body ? "POST" : "GET") => {
    const response = await fetch(new URL("/api/" + path,base),{
      method,headers:{"content-type":"application/json"},body:JSON.stringify(body),
    });
    assert.ok(response.ok, path + " returned " + response.status);
    return response.json();
  };
  await view("home");
  const workspace = await api("workspaces",{title:"Approval fixtures",kind:"analysis"});
  const session = await api("ai/sessions",{title:"Guards",mode:"chat"});
  const boards = {};
  for (const name of ["deny","stale","locked","expired","first","second"]) {
    boards[name] = await api("boards",{parent_kind:"workspace",parent_id:workspace.id,kind:"analysis",title:name,
      start_fen:"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"});
  }
  const seed = name => {
    const board = boards[name];
    const changeset = {summary:name,notes:[],arguments:{board_id:board.id,move:"e4",expected_revision:1},
      board:{board_id:board.id,title:name,expected_revision:1,action:"play_move",before:{},after:{moves:["e4"]}}};
    execFileSync("python3",["-c",`import json, os, sqlite3, sys, time
data=json.loads(sys.argv[1])
with sqlite3.connect(os.environ["RBC_DB"]) as db:
 db.execute("INSERT INTO ai_approvals (id,origin,session_id,target_kind,target_id,action,tool,summary,changeset_json,status,created,expires) VALUES (?,'in_app',?,'board',?,'play_move','play_move',?,?,'pending',?,?)",
 (data["id"],data["session"],data["board"],data["name"],json.dumps(data["changeset"]),int(time.time()*1000),int(time.time()*1000)+60000))
`,JSON.stringify({id:"guard-"+name,session:session.id,board:board.id,name,changeset})]);
  };
  const request = name => api("ai/approvals/guard-" + name);
  const read = name => api("boards/" + boards[name].id);
  const expire = name => execFileSync("python3",["-c",`import os, sqlite3, sys
with sqlite3.connect(os.environ["RBC_DB"]) as db:
 db.execute("UPDATE ai_approvals SET expires=1 WHERE id=?", ("guard-"+sys.argv[1],))
`,name]);
  await destination("workbench");
  await click('[data-open-view="assistant"]');
  await view("assistant");
  await clickText("main button","New chat");
  await clickText("main button","Approvals (0)");
  const refresh = async () => {
    await evaluate('[...document.querySelectorAll("main button")].find(button=>button.textContent.startsWith("Approvals")).click()');
    await evaluate('[...document.querySelectorAll("main button")].find(button=>button.textContent.startsWith("Approvals")).click()');
  };
  const rowAction = (name, action) => evaluate(`(() => {
    const row=[...document.querySelectorAll(".approval-row")].find(row=>row.children[1]?.textContent===${JSON.stringify(name)});
    const button=[...row?.querySelectorAll("button") || []].find(button=>button.textContent.trim()===${JSON.stringify(action)});
    if (!button) throw new Error("approval action missing");
    button.click();
  })()`);
  const shown = name => waitFor(() => evaluate(`[...document.querySelectorAll(".approval-row")].some(row=>row.children[1]?.textContent===${JSON.stringify(name)})`), name + " did not appear in open queue");
  seed("deny");
  await refresh();
  await shown("deny");
  await rowAction("deny","Deny");
  await waitFor(async () => (await request("deny")).status === "denied","deny did not persist");
  assert.equal((await read("deny")).revision,1);

  for (const name of ["stale","locked"]) {
    seed(name);
    await refresh();
    await shown(name);
    if (name === "stale") await api("boards/" + boards[name].id,{expected_revision:1,content_json:'{"notes_md":"new revision"}'},"PUT");
    else await api("boards/" + boards[name].id + "/lock",{});
    const before = await read(name);
    await rowAction(name,"Approve");
    await waitFor(() => evaluate('document.querySelector(".approval-queue")?.textContent.includes("refused")'), name + " write did not report refusal");
    await waitFor(async () => (await request(name)).status === "approved", name + " approval did not resolve");
    assert.deepEqual(await read(name),before,name + " approval changed board");
  }
  seed("expired");
  await refresh();
  await shown("expired");
  expire("expired");
  await rowAction("expired","Approve");
  await waitFor(() => evaluate('document.querySelector(".approval-queue")?.textContent.toLowerCase().includes("expired")'),"expired approval was not refused");
  assert.equal((await read("expired")).revision,1);
  await refresh();

  seed("first");
  seed("second");
  await refresh();
  await shown("first");
  await shown("second");
  await clickText(".approval-queue button","Approve all");
  await waitFor(async () => (await read("first")).revision===2 && (await read("second")).revision===2,"bulk approvals did not apply both writes");
  assert.deepEqual(JSON.parse((await read("first")).content_json).moves,["e4"]);
  assert.deepEqual(JSON.parse((await read("second")).content_json).moves,["e4"]);
  console.log("RBC_APPROVAL_GUARDS_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_APPROVAL_GUARDS_FAIL");
  process.exitCode=1;
});
