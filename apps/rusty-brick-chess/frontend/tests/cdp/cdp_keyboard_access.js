const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  const key = async (key, code = key) => {
    await command("Input.dispatchKeyEvent", {type:"keyDown",key,code,text:key === "Enter" ? "\r" : "",unmodifiedText:key === "Enter" ? "\r" : "",windowsVirtualKeyCode:({Enter:13,Tab:9,Escape:27}[key] || 0)});
    await command("Input.dispatchKeyEvent", {type:"keyUp",key,code,windowsVirtualKeyCode:({Enter:13,Tab:9,Escape:27}[key] || 0)});
  };
  await view("home");
  await evaluate('document.querySelector(".skip-link").focus()');
  await key("Enter");
  assert.equal(await evaluate("document.activeElement.id"), "app-main", "skip link did not focus main");
  await key("Tab");
  assert.ok(await evaluate('document.activeElement.closest("main") !== null'), "skip link did not skip navigation");
  const activate = async selector => {
    await evaluate(`document.querySelector(${JSON.stringify(selector)}).focus()`);
    await key("Enter");
  };
  for (const name of ["home","play","train","workbench","settings"]) {
    await activate(`[data-destination="${name}"]`);
    await view(name);
    assert.equal(await evaluate('getComputedStyle(document.activeElement).outlineStyle !== "none"'),true,"visible keyboard focus missing");
  }
  for (const [group, tools] of [["train",["puzzles","openings","study","practice"]],["workbench",["analysis","database","engines","tournaments","assistant","compare","stats"]]]) {
    for (const tool of tools) {
      await activate(`[data-destination="${group}"]`);
      await view(group);
      await activate(`[data-open-view="${tool}"]`);
      await view(tool);
    }
  }
  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  const focus = name => evaluate(`document.querySelector('.recorded-board [data-square="${name}"]').focus()`);
  await focus("e2");
  await key("Enter");
  await waitFor(() => evaluate('document.querySelector(\'.recorded-board [data-square="e2"]\').classList.contains("selected")'), "keyboard did not select square");
  await focus("e4");
  await key("Enter");
  await waitFor(() => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent.includes("/4P3/")'), "keyboard did not play move");
  await waitFor(() => evaluate('document.activeElement.dataset.square === "e4"'), "move lost keyboard focus");
  const tree = await command("Accessibility.getFullAXTree");
  assert.ok(tree.nodes.some(node => node.role?.value === "button" && node.name?.value?.includes("e4") && node.name.value.includes("pawn")), "board square missing from accessibility tree");
  await setField(".recorded-board .load input", "7k/P7/8/8/8/8/8/7K w - - 0 1");
  await click(".recorded-board .load button");
  await focus("a7");
  await key("Enter");
  await focus("a8");
  await key("Enter");
  await waitFor(() => evaluate('document.activeElement.classList.contains("promo-btn")'), "promotion did not receive focus");
  const promotionTree = await command("Accessibility.getFullAXTree");
  assert.ok(promotionTree.nodes.some(node => node.role?.value === "dialog"), "promotion dialog role missing");
  assert.ok(promotionTree.nodes.some(node => node.role?.value === "button" && node.name?.value?.includes("queen")), "promotion choice unnamed");
  await evaluate('document.querySelector(".promo-btn:last-child").focus()');
  await key("Tab");
  await waitFor(() => evaluate('document.activeElement === document.querySelector(".promo-btn:first-child")'), "Tab escaped the promotion dialog");
  await key("Escape");
  await waitFor(() => evaluate('!document.querySelector(".promo-overlay") && document.activeElement.dataset.square === "a7"'), "cancelled promotion did not restore focus");
  await focus("a8");
  await key("Enter");
  await waitFor(() => evaluate('document.activeElement.classList.contains("promo-btn")'), "promotion did not reopen");
  await key("Enter");
  await waitFor(() => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent.startsWith("Q6k/")'), "keyboard promotion did not apply");
  await waitFor(() => evaluate('document.activeElement.dataset.square === "a8"'), "promotion lost keyboard focus");
  await setField(".ws-new","analysis","change");
  await view("analysis");
  await activate('.ws-tab.active [aria-label="Rename workspace"]');
  await waitFor(()=>evaluate('document.activeElement.classList.contains("ws-rename")'),"workspace rename did not focus its input");
  await setField(".ws-rename","Keyboard workspace");
  await key("Enter");
  await waitFor(()=>evaluate('document.activeElement.classList.contains("ws-name") && document.activeElement.textContent === "Keyboard workspace"'),"workspace rename did not restore focus");
  await destination("settings");
  await view("settings");
  await setField('input[placeholder="Engine name"]',"Keyboard fixture");
  await setField('input[placeholder="/path/to/engine"]',"/fixture-missing-engine");
  await clickText(".settings-form button","Add engine");
  await waitFor(()=>evaluate('!!document.querySelector(".engine-row")'),"engine fixture missing");
  await activate(".engine-row button");
  await waitFor(()=>evaluate('!!document.activeElement.closest(".engine-dialog")'),"engine configuration did not receive keyboard focus");
  await key("Escape");
  await waitFor(()=>evaluate('!document.querySelector(".engine-dialog") && !!document.activeElement.closest(".engine-row")'),"engine dialog did not close and restore focus");
  console.log("RBC_KEYBOARD_ACCESS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_KEYBOARD_ACCESS_FAIL");
  process.exitCode = 1;
});
