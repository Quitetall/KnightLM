// Verify the new destinations through the served WASM app.
const { spawn } = require("node:child_process");
const { once } = require("node:events");
const { mkdtempSync, readFileSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");

const base = process.env.RBC_CDP_URL || "http://127.0.0.1:8090/";
const profile = mkdtempSync(join(tmpdir(), "rbc-shell-"));
const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));

async function waitFor(check, label) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    try { if (await check()) return; } catch (error) {
      if (error.message !== "Runtime.evaluate failed") throw error;
    }
    await sleep(100);
  }
  throw new Error(label);
}

async function main() {
  const browser = spawn("google-chrome", [
    "--headless=new", "--disable-gpu", "--no-sandbox", "--remote-debugging-port=0",
    "--remote-allow-origins=*", "--window-size=1400,900", "--user-data-dir=" + profile, base,
  ], { stdio: "ignore" });
  let ws;
  try {
    let target;
    await waitFor(async () => {
      try {
        const [port] = readFileSync(join(profile, "DevToolsActivePort"), "utf8").split("\n");
        const response = await fetch("http://127.0.0.1:" + port + "/json/list", { signal: AbortSignal.timeout(1000) });
        target = (await response.json()).find(page => page.type === "page" && page.url === base);
        return !!target;
      } catch { return false; }
    }, "browser did not open the app");
    ws = new WebSocket(target.webSocketDebuggerUrl);
    await once(ws, "open");
    let id = 0;
    const pending = new Map();
    ws.onmessage = ({ data }) => {
      const message = JSON.parse(data);
      pending.get(message.id)?.(message);
    };
    const command = (method, params = {}) => new Promise((resolve, reject) => {
      const request = ++id;
      const timer = setTimeout(() => {
        pending.delete(request);
        reject(new Error(method + " timed out"));
      }, 5000);
      pending.set(request, (reply) => {
        clearTimeout(timer);
        pending.delete(request);
        if (reply.error || reply.result?.exceptionDetails) reject(new Error(method + " failed"));
        else resolve(reply.result);
      });
      ws.send(JSON.stringify({ id: request, method, params }));
    });
    const evaluate = async (expression) => (await command("Runtime.evaluate", {
      expression, returnByValue: true, awaitPromise: true,
    })).result.value;
    const click = async selector => {
      if (!await evaluate(`(() => {const item = document.querySelector(${JSON.stringify(selector)}); if (!item) return false; item.click(); return true;})()`)) {
        throw new Error("missing control: " + selector);
      }
    };
    const view = async name => waitFor(
      () => evaluate(`document.querySelector("main")?.dataset.view === ${JSON.stringify(name)}`),
      "view did not open: " + name,
    );
    const destination = name => click(`[data-destination="${name}"]`);
    const screenshot = async name => {
      const evidence = process.env.RBC_CDP_EVIDENCE;
      if (!evidence) return;
      const { data } = await command("Page.captureScreenshot", { format:"png" });
      writeFileSync(join(evidence, name + ".png"), Buffer.from(data, "base64"));
    };
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
  } finally {
    ws?.close();
    if (browser.exitCode === null && browser.signalCode === null) {
      const exited = once(browser, "exit");
      if (browser.kill()) await exited;
    }
    rmSync(profile, { recursive: true, force: true });
  }
}

main().catch(error => {
  console.error(error);
  console.log("RBC_REDESIGN_SHELL_FAIL");
  process.exitCode = 1;
});
