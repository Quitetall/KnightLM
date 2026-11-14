const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, destination, view, setField, command }) => {
  await view("home");
  await evaluate(`(async () => {
    const post = async (path,body) => {
      const response = await fetch(path,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(body)});
      if (!response.ok) throw new Error(await response.text());
      return response.json();
    };
    const workspace = await post("/api/workspaces",{title:"References",kind:"analysis"});
    window.refBoard = await post("/api/boards",{parent_kind:"workspace",parent_id:workspace.id,kind:"analysis",title:"MentionBoard",start_fen:"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"});
    window.refStudy = await post("/api/studies",{title:"MentionStudy"});
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await original(...args);
      const path = new URL(args[0].url || args[0],location.href).pathname;
      if (path === "/api/studies") await new Promise(resolve=>{window.releaseReferences = resolve});
      return response;
    };
  })()`);
  await destination("workbench");
  await click('[data-open-view="assistant"]');
  await view("assistant");
  await setField(".composer","@MentionBoard");
  await waitFor(() => evaluate('!!window.releaseReferences'),"references request did not arrive");
  await evaluate("window.releaseReferences()");
  try {
    await waitFor(() => evaluate('!!document.querySelector(".composer-palette.mention .item")'),"references did not refresh palette");
  } catch (error) {
    console.log(await evaluate(`JSON.stringify({draft:document.querySelector(".composer").value,
      board:window.refBoard,study:window.refStudy,
      palettes:[...document.querySelectorAll(".composer-palette")].map(list=>list.textContent)})`));
    throw error;
  }
  await click(".composer-palette.mention .item");
  assert.equal(await evaluate('document.querySelector(".context-chip")?.dataset.kind'),"board");
  await setField(".composer","@MentionStudy");
  await waitFor(() => evaluate('!!document.querySelector(".composer-palette.mention .item")'),"study mention missing");
  await evaluate('document.querySelector(".composer").focus()');
  await command("Input.dispatchKeyEvent",{type:"keyDown",key:"Enter",code:"Enter",text:"\r",windowsVirtualKeyCode:13});
  await command("Input.dispatchKeyEvent",{type:"keyUp",key:"Enter",code:"Enter",windowsVirtualKeyCode:13});
  assert.equal(await evaluate('document.querySelectorAll(".context-chip").length'),2);
  console.log("RBC_COMPOSER_REFERENCES_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_COMPOSER_REFERENCES_FAIL");
  process.exitCode=1;
});
