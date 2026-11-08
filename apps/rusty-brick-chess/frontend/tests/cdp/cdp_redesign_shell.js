// Verify the new destinations through the served WASM app.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, view, destination, screenshot }) => {
    await view("home");
    await waitFor(() => evaluate(`document.querySelectorAll("[data-destination]").length === 5
      && !document.querySelector(".topbar input") && !!document.querySelector(".workspaces")`), "unexpected launch shell");
    await screenshot("home-desktop");
    await destination("play");
    await view("play");
    for (const [group, sections] of [
      ["train", ["puzzles", "openings", "study", "practice"]],
      ["workbench", ["analysis", "database", "engines", "tournaments", "assistant", "compare", "stats"]],
    ]) {
      for (const section of sections) {
        await destination(group);
        await click(`[data-open-view="${section}"]`);
        await view(section);
        await waitFor(() => evaluate(`document.querySelector('[aria-current="page"]')?.dataset.destination === ${JSON.stringify(group)}
          && !!document.querySelector(".workspaces") && !!document.querySelector(".topbar input") === ${section === "database"}`),
          "incorrect shell or search scope for " + section);
      }
    }
    await destination("workbench");
    await click('[data-open-view="database"]');
    await view("database");
    const seeded = await evaluate(`(async () => {
      const response = await fetch("/api/games", {
        method:"POST", headers:{"content-type":"application/json"},
        body:JSON.stringify({pgn:'[White "ParityAlice"]\\n[Black "ParityBob"]\\n[Result "*"]\\n\\n1. e4 *'}),
      });
      return (await response.json()).imported?.[0];
    })()`);
    if (!seeded) throw new Error("could not seed search fixture");
    const search = async (name, enter) => evaluate(`(() => {
      const input = document.querySelector(".topbar input");
      input.value = ${JSON.stringify(name)};
      input.dispatchEvent(new Event("input", {bubbles:true}));
      if (${enter}) input.dispatchEvent(new KeyboardEvent("keydown", {key:"Enter",bubbles:true}));
      else document.querySelector(".topbar button").click();
    })()`);
    const matchingGame = () => evaluate(`[...document.querySelectorAll("[data-game-id]")].map(row => Number(row.dataset.gameId)).join(",") === "${seeded}"`);
    await search("ParityAlice", false);
    await waitFor(matchingGame, "search button missed the fixture");
    await search("NoMatchingPlayer", false);
    await waitFor(() => evaluate('!document.querySelector("[data-game-id]")'), "filter did not clear previous result");
    await search("ParityAlice", true);
    await waitFor(matchingGame, "Enter missed the fixture");
    await destination("settings");
    await view("settings");
    await command("Page.reload");
    await waitFor(() => evaluate(`performance.getEntriesByType("navigation")[0]?.type === "reload"`), "reload did not complete");
    await view("settings");
    await command("Page.navigate", { url: base + "?cold-launch" });
    await view("home");
    const workspaceNames = () => evaluate(`[...document.querySelectorAll(".ws-name")].map(tab => tab.textContent.trim())`);
    const kinds = [["analysis", "analysis"], ["database", "database"], ["openings", "openings"], ["puzzles", "puzzles"]];
    for (const [index, [kind, expected]] of kinds.entries()) {
      await evaluate(`(() => {
        const picker = document.querySelector(".ws-new");
        picker.value = ${JSON.stringify(kind)};
        picker.dispatchEvent(new Event("change", {bubbles:true}));
      })()`);
      await view(expected);
      await waitFor(async () => (await workspaceNames()).length === index + 1, "workspace was not added: " + kind);
    }
    await click(".ws-tab .ws-name");
    await view("analysis");
    const rename = async title => evaluate(`(() => {
      const input = document.querySelector(".ws-rename");
      input.value = ${JSON.stringify(title)};
      input.dispatchEvent(new Event("input", {bubbles:true}));
      input.dispatchEvent(new KeyboardEvent("keydown", {key:"Enter",bubbles:true}));
    })()`);
    await click('.ws-tab.active [aria-label="Rename workspace"]');
    await rename("Parity analysis");
    await waitFor(async () => (await workspaceNames())[0] === "Parity analysis", "rename control did not save");
    await evaluate(`document.querySelector(".ws-tab.active .ws-name").dispatchEvent(new MouseEvent("dblclick",{bubbles:true}))`);
    await rename("Renamed again");
    await waitFor(async () => (await workspaceNames())[0] === "Renamed again", "double-click rename did not save");
    await click('.ws-tab.active [aria-label="Move workspace right"]');
    await waitFor(async () => (await workspaceNames())[1] === "Renamed again", "move right did not reorder");
    await click('.ws-tab.active [aria-label="Move workspace left"]');
    await waitFor(async () => (await workspaceNames())[0] === "Renamed again", "move left did not reorder");
    await click(".ws-tab:not(.active) .ws-close");
    await waitFor(async () => (await workspaceNames()).length === 3, "inactive workspace did not close");
    await view("analysis");
    await click(".ws-tab.active .ws-close");
    await waitFor(async () => (await workspaceNames()).length === 2, "active workspace did not close");

    await command("Emulation.setDeviceMetricsOverride", {
      width: 390, height: 844, deviceScaleFactor: 1, mobile: true,
    });
    await waitFor(() => evaluate(`innerWidth <= 400 && [...document.querySelectorAll("[data-destination]")].every(button => {
      const bounds = button.getBoundingClientRect();
      return bounds.left >= 0 && bounds.right <= innerWidth;
    })`), "main navigation clipped at 390px");
    await destination("workbench");
    await view("workbench");
    await screenshot("workbench-narrow");
    await click('[data-open-view="database"]');
    await view("database");
    console.log("RBC_REDESIGN_SHELL_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_REDESIGN_SHELL_FAIL");
  process.exitCode = 1;
});
