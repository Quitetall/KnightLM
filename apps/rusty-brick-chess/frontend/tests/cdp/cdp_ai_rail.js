const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, clickText, setField, destination, view, click }) => {
  const assistant = async () => {
    await destination("workbench");
    await click('[data-open-view="assistant"]');
    await view("assistant");
  };
  await view("home");
  await assistant();
  await clickText("button", "New chat");
  await waitFor(() => evaluate('!!document.querySelector(".dock-tab.active")'), "new chat did not appear");
  const before = await evaluate('document.querySelectorAll(".dock-tab").length');
  assert.ok(before > 0);
  await setField('input[type="search"]', "zzz-no-such-chat");
  await waitFor(() => evaluate('document.querySelectorAll(".dock-tab").length === 0'), "search did not filter chats");
  await setField('input[type="search"]', "");
  await waitFor(() => evaluate(`document.querySelectorAll(".dock-tab").length === ${before}`), "clearing search did not restore chats");
  const selected = await evaluate('document.querySelector(".dock-tab.active").textContent.trim()');
  await command("Page.reload");
  await waitFor(() => evaluate('!!document.querySelector("[data-destination]")'), "shell did not reload");
  await assistant();
  await waitFor(() => evaluate(`document.querySelector(".dock-tab.active")?.textContent.trim() === ${JSON.stringify(selected)}`), "selected chat did not survive reload");
  console.log("AI_RAIL_PASS");
}).catch(error => { console.error(error); console.log("AI_RAIL_FAIL"); process.exitCode = 1; });
