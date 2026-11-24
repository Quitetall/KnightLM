// RBCUI-CFG-01. Configure real UCI engines and preserve previously saved options.
const { symlinkSync } = require("node:fs");
const { join } = require("node:path");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, destination, setField, view }) => {
  const stockfish = process.env.STOCKFISH_PATH;
  if (!stockfish) throw new Error("Set STOCKFISH_PATH to a local UCI engine");
  const engine = join(process.env.TMPDIR, "configured-stockfish");
  symlinkSync(stockfish, engine);
  const invalid = join(process.env.TMPDIR, "missing-engine");
  const config = () => evaluate('fetch("/api/settings").then(response => response.json())');
  const defaultPath = 'input[placeholder="/path/to/stockfish"]';
  await view("home");
  // Simulate the names written by the prior English and Spanish controls.
  const seeded = await evaluate(`fetch("/api/settings", {
    method:"PUT",headers:{"content-type":"application/json"},
    body:JSON.stringify({engines:[{name:"Legacy fixture",path:${JSON.stringify(invalid)},
      options:{"Hilos":"3","Lines to analyse":"4","Líneas a analizar":"5","MultiPV":"2"}}]})
  }).then(r=>r.ok)`);
  if (!seeded) throw new Error("could not seed legacy engine options");
  await command("Page.reload");
  await view("home");
  await destination("workbench");
  await click('[data-open-view="engines"]');
  await view("engines");
  await clickText(".destination-grid button", "Configure");
  await view("settings");
  await waitFor(() => evaluate('document.querySelector(".engine-list")?.textContent.includes("Legacy fixture")'), "legacy engine did not load");
  await clickText(".engine-row button", "Configure");
  const dialogFields = ".engine-dialog .engine-fields:nth-of-type(3) input";
  const values = () => evaluate(`[...document.querySelectorAll(${JSON.stringify(dialogFields)})].map(input => input.value)`);
  if ((await values()).slice(1,3).join(",") !== "3,2") throw new Error("legacy options did not load with canonical precedence");
  await clickText(".engine-dialog button", "Test engine");
  await waitFor(() => evaluate('document.querySelector(".engine-test .error")?.textContent.includes("could not start engine")'), "failed UCI handshake did not show an error");
  await clickText(".engine-dialog button", "Close");

  await setField('input[placeholder="Engine name"]', "New fixture");
  await setField('input[placeholder="/path/to/engine"]', stockfish);
  await clickText(".settings-form button", "Add engine");
  await waitFor(() => evaluate('document.querySelectorAll(".engine-row").length === 2'), "second engine was not added");
  await clickText(".engine-row:nth-child(2) button", "Configure");
  await setField(".engine-dialog .engine-fields:nth-of-type(2) label:nth-child(1) input", "Configured fixture", "change");
  await setField(".engine-dialog .engine-fields:nth-of-type(2) label:nth-child(2) input", engine, "change");
  await clickText(".engine-dialog button", "Test engine");
  await waitFor(() => evaluate('document.querySelector(".engine-test .best")?.textContent.toLowerCase().includes("stockfish")'), "real UCI handshake failed");
  await clickText(".engine-dialog button", "Close");

  await setField('select:has(option[value="es"])', "es", "change");
  await clickText(".engine-row:nth-child(2) button", "Configurar");
  const options = {Hash:"32",Threads:"2",MultiPV:"3","Skill Level":"12",UCI_Elo:"1600",SyzygyPath:process.env.TMPDIR};
  for (const [index, value] of Object.values(options).entries()) {
    await setField(`.engine-dialog .engine-fields:nth-of-type(3) label:nth-child(${index + 1}) input`, value, "change");
  }
  await clickText(".engine-dialog button", "Guardar");
  await waitFor(async () => {
    const entry = (await config()).engines[1];
    return entry?.name === "Configured fixture" && entry.path === engine &&
      Object.keys(entry.options).length === Object.keys(options).length &&
      Object.entries(options).every(([key,value]) => entry.options[key] === value);
  }, "translated engine controls did not persist canonical UCI options");
  await clickText(".engine-dialog button", "Cerrar");
  await setField('select:has(option[value="es"])', "en", "change");
  await clickText(".engine-row:nth-child(2) button", "Use");
  await waitFor(async () => (await config()).engine_path === engine, "selected engine did not become active");
  await setField(defaultPath, engine);
  await setField('.settings-form input[type="number"][max="30"]', "5");
  await setField('.settings-form input[type="number"][max="256"]', "1");
  await setField('.settings-form input[type="number"][max="65536"]', "64");
  await setField('input[placeholder="/path/to/syzygy"]', process.env.TMPDIR);
  await setField('input[placeholder="/path/to/fairy-stockfish"]', engine);
  await clickText(".settings-form button", "Save");
  await waitFor(async () => {
    const settings = await config();
    return settings.engine_path === engine && settings.depth === 5 && settings.threads === 1 &&
      settings.hash_mb === 64 && settings.syzygy_path === process.env.TMPDIR &&
      settings.variant_engine_path === engine;
  }, "global engine configuration did not persist");
  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await clickText(".simple-analysis button", "Analyze");
  await waitFor(() => evaluate('!!document.querySelector(".simple-analysis p")?.textContent'), "selected engine did not produce a suggestion");
  await destination("settings");
  await view("settings");
  await clickText(".engine-row:nth-child(1) button", "Remove");
  await waitFor(async () => (await config()).engines.map(entry => entry.name).join(",") === "Configured fixture", "wrong engine removed");
  await command("Page.reload");
  await view("settings");
  await waitFor(() => evaluate('document.querySelectorAll(".engine-row").length === 1 && document.querySelector(".engine-name.active")?.textContent.includes("Configured fixture")'), "engine selection did not survive reload");
  console.log("RBC_ENGINE_SETTINGS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ENGINE_SETTINGS_FAIL");
  process.exitCode = 1;
});
