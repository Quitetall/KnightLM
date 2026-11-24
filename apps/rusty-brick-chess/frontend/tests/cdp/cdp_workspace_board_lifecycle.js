// RBCUI-SHELL-06. Verify board identity, content, revision and parent across UI operations.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const listWorkspaces = () => evaluate('fetch("/api/workspaces").then(response => response.json())');
  const listBoards = id => evaluate(`fetch("/api/workspaces/${id}/boards").then(response => response.json())`);
  const createWorkspace = () => setField(".ws-new", "analysis", "change");
  await createWorkspace();
  await view("analysis");
  await waitFor(async () => (await listWorkspaces()).length === 1, "source workspace not saved");
  const source = (await listWorkspaces())[0].id;
  await createWorkspace();
  await waitFor(async () => (await listWorkspaces()).length === 2, "destination workspace not saved");
  const target = (await listWorkspaces()).find(item => item.id !== source).id;
  await click(".ws-tab .ws-name");
  await click('[data-analysis-view="detailed"]');

  const fen = "8/8/8/8/8/2k5/5R2/7K w - - 0 1";
  await setField(".recorded-board .load input", fen);
  await click(".recorded-board .load button");
  await waitFor(() => evaluate(`document.querySelector(".recorded-board .fen-line code")?.textContent === "${fen}"`), "fixture position missing");
  await clickText(".boards button", "New board");
  await waitFor(async () => (await listBoards(source)).length === 1, "source board not created");
  const original = (await listBoards(source))[0];
  if (original.start_fen !== fen) throw new Error("new board lost the visible position");
  await waitFor(() => evaluate('!!document.querySelector(".board-chip")'), "board chip not visible");
  await click(".board-chip");
  await clickText(".boards button", "Copy");
  await waitFor(async () => (await listBoards(source)).length === 2, "copy not created");
  const copy = (await listBoards(source)).find(item => item.id !== original.id);
  if (copy.start_fen !== original.start_fen || copy.content_json !== original.content_json) throw new Error("copy content differed");

  await waitFor(() => evaluate('document.querySelectorAll(".board-chip").length === 2'), "copy chip not refreshed");
  await evaluate('[...document.querySelectorAll(".board-chip")].at(-1).click()');
  await clickText(".boards button", "Lock");
  const load = id => evaluate(`fetch("/api/boards/${id}").then(response => response.json())`);
  await waitFor(async () => (await load(copy.id)).mutability === "locked", "copy not locked");
  const locked = await load(copy.id);
  const denied = await evaluate(`fetch("/api/boards/${copy.id}", {
    method:"PUT", headers:{"content-type":"application/json"}, body:JSON.stringify({expected_revision:${locked.revision},content_json:'{"moves":["Rf8"]}'}),
  }).then(response => response.status)`);
  const after = await load(copy.id);
  if (denied < 400 || after.revision !== locked.revision || after.content_json !== locked.content_json) throw new Error("locked write changed content or revision");
  await waitFor(() => evaluate('document.querySelector(".board-chip.active")?.textContent.includes("locked")'), "lock not reflected in the UI");
  const moveTarget = await evaluate('[...document.querySelector(".board-move-target").options].find(option => option.value)?.value');
  if (!moveTarget) throw new Error("workspace destination missing");
  await setField(".board-move-target", moveTarget, "change");
  await clickText(".boards button", "Move");
  await waitFor(() => evaluate('document.querySelector(".boards-msg")?.textContent.includes("copy_required") || /locked|copy/i.test(document.querySelector(".boards-msg")?.textContent)'), "locked move did not show refusal");
  await clickText(".boards button", "Unlock");
  await waitFor(async () => (await load(copy.id)).mutability === "mutable", "copy not unlocked");
  await waitFor(() => evaluate('!document.querySelector(".board-chip.active")?.textContent.includes("locked")'), "unlock did not refresh");
  await setField(".board-move-target", moveTarget, "change");
  await clickText(".boards button", "Move");
  await waitFor(async () => (await listBoards(target)).some(item => item.id === copy.id) && (await listBoards(source)).length === 1, "board move did not update both parents");
  const moved = await load(copy.id);
  if (moved.content_json !== copy.content_json || moved.start_fen !== fen || moved.revision <= copy.revision) throw new Error("moved board did not retain content and advance revision");
  await waitFor(() => evaluate(`(() => {
    const entries = [...document.querySelectorAll(".board-tree .tree-board")];
    const entry = entries.find(button => button.textContent.includes("copy") && button.textContent.includes("r${moved.revision}"));
    if (!entry) return false;
    entry.closest("details").open = true;
    entry.click();
    return true;
  })()`), "moved board or revision missing from navigator");
  await waitFor(() => evaluate(`document.querySelector(".board-chip.active")?.textContent.includes("r${moved.revision}")`), "navigator did not select moved board");
  console.log("RBC_WORKSPACE_BOARD_LIFECYCLE_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_WORKSPACE_BOARD_LIFECYCLE_FAIL");
  process.exitCode = 1;
});
