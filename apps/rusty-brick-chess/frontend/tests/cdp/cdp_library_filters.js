// RBCUI-LIB-03. Check catalog, filter, sort, exact/partial positions and preview-only games.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const specs = [
    ["Zephyr","Basil","2024.04.01",2100,"1-0","","Shared fixture","1. e4 e5 2. Nf3 Nc6"],
    ["Atlas","Zephyr","2022.02.01",1500,"0-1","","Shared fixture","1. d4 d5"],
    ["Mantis","Alpha","2025.02.01",2000,"*","Atomic","Atomic fixture","1. e4 e5 2. Nf3"],
    ["Boreal","Cedar","2023.02.01",1000,"*","Bughouse","Unsupported fixture","1. c4"],
  ];
  const ids = await evaluate(`(async () => {
    const specs = ${JSON.stringify(specs)};
    const pgn = specs.map(([white,black,date,elo,result,variant,event,moves]) =>
      [["White",white],["Black",black],["Date",date],["WhiteElo",elo],["BlackElo",elo],["Result",result],["Event",event],...(variant?[["Variant",variant]]:[])]
        .map(([key,value])=>'['+key+' "'+value+'"]').join("\\n")+"\\n\\n"+moves+" "+result
    ).join("\\n\\n");
    const response = await fetch("/api/games",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({pgn})});
    return (await response.json()).imported;
  })()`);
  if (ids?.length !== 4) throw new Error("filter fixtures did not import");
  const [a,b,c,d] = ids;
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await click(".topbar button");

  const rows = () => evaluate('[...document.querySelectorAll("[data-game-id]")].map(row=>Number(row.dataset.gameId))');
  const expect = (expected, label) => waitFor(async () => (await rows()).join(",") === expected.join(","), label);
  const apply = () => click(".filters button");
  const clear = () => clickText(".main button", "Clear filters");
  const sort = '.filters select:has(option[value="oldest"])';
  const variant = '.filters select:has(option[value="atomic"])';
  const result = '.filters select:has(option[value="1-0"])';
  await expect([d,c,b,a], "newest ordering wrong");
  for (const [kind,order] of [
    ["oldest",[a,b,c,d]],["date",[c,a,d,b]],["white",[b,d,c,a]],
    ["black",[c,a,d,b]],["length",[a,c,b,d]],["newest",[d,c,b,a]],
  ]) {
    await setField(sort, kind, "change");
    await apply();
    await expect(order, kind + " ordering wrong");
  }

  await waitFor(() => evaluate('document.querySelector(".catalog-row")?.textContent.includes("Zephyr")'), "player catalog missing");
  await clickText(".catalog button", "Zephyr (2)");
  await expect([b,a], "player catalog did not filter both seats");
  await clear();
  await clickText(".catalog button", "Shared fixture (2)");
  await expect([b,a], "event catalog did not filter");
  await clickText(".main button", "clear");
  await expect([d,c,b,a], "event clear did not reset results");

  await setField('.filters input[type="date"]:first-of-type', "2025-01-01", "change");
  await apply();
  await expect([c], "start date not applied");
  await clear();
  await setField('.filters label:nth-child(2) input[type="date"]', "2022-12-31", "change");
  await apply();
  await expect([b], "end date not applied");
  await clear();
  await setField('.filters input[placeholder="min"]', "1800");
  await setField('.filters input[placeholder="max"]', "2050");
  await apply();
  await expect([c], "Elo range not applied");
  await clear();
  await setField(result, "0-1", "change");
  await apply();
  await expect([b], "result not applied");
  await clear();
  await setField(variant, "atomic", "change");
  await apply();
  await expect([c], "variant not applied");
  await clear();
  await setField('.filters label:nth-child(1) input[type="date"]', "2024-01-01", "change");
  await setField('.filters label:nth-child(2) input[type="date"]', "2024-12-31", "change");
  await setField('.filters input[placeholder="min"]', "2000");
  await setField('.filters input[placeholder="max"]', "2200");
  await setField(result, "1-0", "change");
  await setField(variant, "standard", "change");
  await apply();
  await expect([a], "combined filter did not intersect all fields");
  await clear();
  await expect([d,c,b,a], "clear did not restore all games");

  const search = '.load:has(select option[value="atomic"])';
  const e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
  await setField(search + " input[type=text]", e4);
  await clickText(search + " button", "Find position");
  await expect([a], "standard FEN search included another variant");
  await setField(search + " select", "atomic", "change");
  await clickText(search + " button", "Find position");
  await expect([c], "Atomic FEN not indexed separately");
  await click(search + ' input[type="checkbox"]');
  await setField(search + " input[type=text]", "8/8/8/8/4P3/8/8/8");
  await clickText(search + " button", "Find position");
  await waitFor(async () => (await rows()).sort((x,y)=>x-y).join(",") === [a,c].join(","), "partial position did not find both games");
  await clear();
  await expect([d,c,b,a], "clear did not remove position results");

  await click(`[data-game-id="${d}"]`);
  await view("analysis");
  await waitFor(() => evaluate('document.querySelector(".topbar .status")?.textContent.includes("preview only")'), "unsupported game did not open in preview");
  const before = await evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent');
  const square = async name => {
    const point = await evaluate(`(() => {
      const rect = document.querySelector('.recorded-board [data-square="${name}"]').getBoundingClientRect();
      return {x:rect.left + rect.width / 2,y:rect.top + rect.height / 2};
    })()`);
    for (const type of ["mousePressed","mouseReleased"]) {
      await command("Input.dispatchMouseEvent",{type,...point,button:"left",clickCount:1});
    }
  };
  await square("e2");
  await square("e4");
  if (await evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent') !== before) throw new Error("preview-only game accepted a move");
  console.log("RBC_LIBRARY_FILTERS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_LIBRARY_FILTERS_FAIL");
  process.exitCode = 1;
});
