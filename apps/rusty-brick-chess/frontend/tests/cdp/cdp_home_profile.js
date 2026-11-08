// RBCUI-SHELL-02. Home actions, display name persistence and recent games.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const game = await evaluate(`(async () => {
    const pgn = '[White "Recent fixture"]\\n[Black "Opponent"]\\n[Event "Parity event"]\\n[Result "*"]\\n\\n1. d4 *';
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify({pgn}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  if (!game) throw new Error("could not seed recent game");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector(".home .row.game")?.textContent.includes("Recent fixture")'), "recent game missing after reload");
  await waitFor(() => evaluate(`[...document.querySelectorAll(".home-stat")].map(stat => stat.textContent.trim()).join(" ").replace(/\\s+/g, "").includes("1games")`), "Home game count missing");
  await click(".home details summary");
  await setField(".home-hero input", "Parity Player");
  await clickText(".home-hero button", "Save profile");
  await waitFor(() => evaluate('fetch("/api/settings").then(response => response.json()).then(data => data.profile_name === "Parity Player")'), "display name was not saved");
  await command("Page.reload");
  await view("home");
  await waitFor(() => evaluate('document.querySelector(".home-greeting")?.textContent.includes("Parity Player")'), "greeting lost display name");
  await clickText(".home .row.game", await evaluate('document.querySelector(".home .row.game").textContent.trim()'));
  await view("analysis");
  await waitFor(() => evaluate(`!!document.querySelector('a[href="/api/games/${game}/pgn"]')`), "recent game opened the wrong game");

  for (const [label, page] of [
    ["Import / browse games","database"], ["New analysis board","analysis"],
    ["Solve a puzzle","puzzles"], ["Train a repertoire","openings"], ["Settings","settings"],
  ]) {
    await destination("home");
    await view("home");
    await clickText(".home button", label);
    await view(page);
    if (page === "openings") {
      await waitFor(() => evaluate('document.querySelector(".panel-head .chip.active")?.textContent.includes("Repertoire")'), "quick action did not enter repertoire mode");
    }
  }
  console.log("RBC_HOME_PROFILE_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_HOME_PROFILE_FAIL");
  process.exitCode = 1;
});
