// Board target markers, off-screen target badge, and the sidebar board tree.
const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ evaluate, click, clickText, setField, destination, view }) => {
  const open = async name => {
    await destination("workbench");
    await click(`[data-open-view="${name}"]`);
    await view(name);
  };
  await view("home");
  await open("analysis");
  await evaluate(`(() => {
    const original=window.fetch.bind(window);
    window.fetch=async (...args)=>{
      const request=args[0] instanceof Request ? args[0] : new Request(...args);
      const response=await original(...args);
      if (request.method==="POST" && new URL(request.url).pathname==="/api/workspaces") {
        while (!window.releaseWorkspaceReply) await new Promise(resolve=>setTimeout(resolve,50));
      }
      return response;
    };
  })()`);
  await setField("select.ws-new", "analysis", "change");
  assert.ok(await evaluate('[...document.querySelectorAll(".boards button")].find(button=>button.textContent.trim()==="New board")?.disabled'), "New board must wait for workspace creation");
  await evaluate('window.releaseWorkspaceReply=true');
  await waitFor(() => evaluate('[...document.querySelectorAll(".boards button")].some(button=>button.textContent.trim()==="New board" && !button.disabled)'), "workspace board controls missing");
  for (let i=0; i<3; i++) {
    const before = await evaluate('document.querySelectorAll(".board-chip").length');
    await clickText(".boards button", "New board");
    await waitFor(() => evaluate(`document.querySelectorAll(".board-chip").length === ${before+1}`), "new board did not appear");
  }
  await open("assistant");
  await clickText("button", "New chat");
  await waitFor(() => evaluate('!!document.querySelector(".dock-tab.active")'), "target session did not appear");
  await open("analysis");
  assert.ok(await evaluate('document.querySelectorAll(".board-chip").length >= 2'));
  await click(".board-set-target");
  await waitFor(() => evaluate('document.querySelectorAll(".board-chip.target").length === 1'), "target marker missing");
  assert.ok(await evaluate('document.querySelector(".board-chip.target").textContent.includes("AI target")'));
  await evaluate('[...document.querySelectorAll(".board-chip")].find(board=>!board.classList.contains("active")).click()');
  await waitFor(() => evaluate('document.querySelector(".board-target-badge")?.textContent.includes("AI → ")'), "off-screen target badge missing");
  await click(".board-target-badge");
  await waitFor(() => evaluate('!document.querySelector(".board-target-badge") && document.querySelector(".board-chip.active")?.classList.contains("target")'), "badge did not focus target board");
  const target = await evaluate('document.querySelector(".board-chip.active").textContent.trim()');
  assert.ok(target.includes("AI target"));
  assert.ok(await evaluate('document.querySelectorAll(".board-tree .tree-board").length >= 2 && document.querySelectorAll(".board-tree .tree-ws").length >= 1'));
  await evaluate('[...document.querySelectorAll(".board-tree .tree-board")].find(board=>!board.classList.contains("active")).click()');
  await waitFor(() => evaluate(`!!document.querySelector(".board-chip.active") && document.querySelector(".board-chip.active").textContent.trim() !== ${JSON.stringify(target)}`), "sidebar tree did not select the other board");
  console.log("AI_CROSS_BOARD_PASS");
}).catch(error => { console.error(error); console.log("AI_CROSS_BOARD_FAIL"); process.exitCode = 1; });
