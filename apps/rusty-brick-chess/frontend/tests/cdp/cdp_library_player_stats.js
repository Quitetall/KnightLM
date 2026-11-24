// RBCUI-LIB-05. Verify local profile and per-database statistics against known results.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const seed = games => evaluate(`(async () => {
    const pgn = ${JSON.stringify(games)}.map(([white,black,result,event],index)=>
      '[White "'+white+'"]\\n[Black "'+black+'"]\\n[Result "'+result+'"]\\n[Event "'+event+'"]\\n\\n1. '+["e4","d4","c4","Nf3"][index]+' '+result
    ).join("\\n\\n");
    const response = await fetch("/api/games",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({pgn})});
    return (await response.json()).imported?.length;
  })()`);
  if (await seed([
    ["Profile Player","First","1-0","Event A"],
    ["Second","Profile Player","1/2-1/2","Event A"],
    ["Third","Profile Player","1-0","Event B"],
    ["Profile Player","Fourth","*","Event B"],
  ]) !== 4) throw new Error("profile games not imported");
  const collection = await evaluate('fetch("/api/collections",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({name:"Fixture collection"})}).then(response=>response.ok)');
  if (!collection) throw new Error("collection fixture not created");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && document.querySelector("main")?.dataset.view === "home"'), "app did not reload");
  const homeProfile = async () => {
    await destination("home");
    await view("home");
    await evaluate('document.querySelector(".home details").open = true');
  };
  const profileSelect = '.home details select:has(option[value="Profile Player"])';
  const select = async () => {
    await waitFor(() => evaluate(`!!document.querySelector(${JSON.stringify(profileSelect)})`), "profile player missing");
    await setField(profileSelect, "Profile Player", "change");
    await clickText(".home details button", "Use as my account");
    await waitFor(() => evaluate('fetch("/api/settings").then(response=>response.json()).then(data=>data.profile_player === "Profile Player")'), "profile selection was not saved");
  };
  const profile = async expected => {
    try {
      await waitFor(() => evaluate(`document.querySelector(".profile-card")?.textContent.includes(${JSON.stringify(expected)})`), "profile value missing: " + expected);
    } catch (error) {
      console.log(JSON.stringify({rendered:await evaluate('document.querySelector(".profile-card")?.textContent'), expected, games:await evaluate('fetch("/api/games").then(r=>r.json()).then(d=>d.games?.map(g=>[g.white,g.black,g.result]))')}));
      throw error;
    }
  };
  await homeProfile();
  await select();
  for (const item of ["Profile Player","1W · 1D · 1L","50.0% score · 3 games","2 as White · 2 as Black · 1 unfinished"]) await profile(item);

  const library = async () => {
    await destination("workbench");
    await click('[data-open-view="database"]');
    await view("database");
  };
  const stats = async expected => {
    await clickText("main button", "Statistics");
    await view("stats");
    await waitFor(() => evaluate(`(() => {
      const found = Object.fromEntries([...document.querySelectorAll(".stat-card")].map(card=>[
        card.querySelector(".stat-label").textContent.trim(), Number(card.querySelector(".stat-value").textContent)
      ]));
      return Object.entries(${JSON.stringify(expected)}).every(([key,value])=>found[key]===value) && found.Puzzles > 0;
    })()`), "database statistics did not match fixture");
  };
  await library();
  const dbSelector = '.dbs:has(input[placeholder="New database…"]) select';
  const original = await evaluate(`document.querySelector(${JSON.stringify(dbSelector)})?.value`);
  await stats({Games:4,"White wins":2,Draws:1,"Black wins":0,Unfinished:1,Players:5,Events:2,Collections:1});
  await waitFor(() => evaluate('document.querySelector(".catalog-row")?.textContent.includes("Profile Player — 4 games")'), "most active players missing");

  await library();
  await setField('input[placeholder="New database…"]', "Profile alternate");
  await clickText('.dbs:has(input[placeholder="New database…"]) button', "Create");
  await waitFor(() => evaluate(`document.querySelector(${JSON.stringify(dbSelector)})?.value && document.querySelector(${JSON.stringify(dbSelector)})?.value !== ${JSON.stringify(original)}`), "second database not selected");
  if (await seed([["Fifth","Profile Player","0-1","Second DB"]]) !== 1) throw new Error("second database fixture did not import");
  await click(".topbar button");
  await waitFor(() => evaluate('document.querySelector("[data-game-id]")?.textContent.includes("Fifth")'), "second game list not refreshed");
  await stats({Games:1,"White wins":0,Draws:0,"Black wins":1,Unfinished:0,Players:2,Events:1,Collections:0});
  await homeProfile();
  await select();
  await profile("1W · 0D · 0L");
  await library();
  await setField(dbSelector, original, "change");
  await waitFor(() => evaluate(`document.querySelector(${JSON.stringify(dbSelector)})?.value === ${JSON.stringify(original)} && document.querySelectorAll("[data-game-id]").length === 4`), "original database did not restore");
  await homeProfile();
  await waitFor(() => evaluate(`document.querySelector(${JSON.stringify(profileSelect)})?.value === "Profile Player"`), "saved profile was not found after switching back");
  await profile("1W · 1D · 1L");
  console.log("RBC_LIBRARY_PLAYER_STATS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_LIBRARY_PLAYER_STATS_FAIL");
  process.exitCode = 1;
});
