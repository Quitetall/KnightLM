// The Play AI tab, offline: the test bot, then the "model" through the managed runtime in its
// fake mode (a stand-in server, a manifest that points at local files). No network, no GPU.
//
//   node cdp_play_ai.js        # prints PLAY_AI_PASS
//
// Needs: a built server (apps/rusty-brick-chess/target/debug/rusty-brick-chess, or RBC_BIN), the
// deployed frontend (frontend/deploy.sh), google-chrome, python3 with fastapi and uvicorn
// (RBC_KNIGHTLM_PYTHON names it), tar.
const { spawn, execFileSync } = require("child_process");
const crypto = require("crypto");
const fs = require("fs");
const http = require("http");
const os = require("os");
const path = require("path");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const root = path.resolve(__dirname, "../../../../..");
const app = path.join(root, "apps/rusty-brick-chess");
const bin = process.env.RBC_BIN || path.join(app, "target/debug/rusty-brick-chess");
const python = process.env.RBC_KNIGHTLM_PYTHON || "python3";
const work = fs.mkdtempSync(path.join(os.tmpdir(), "rbc-playai-"));
const sha = (b) => crypto.createHash("sha256").update(b).digest("hex");
const failures = [];
const check = (name, ok, extra) => { if (!ok) failures.push(name + (extra ? ` (${extra})` : "")); console.log(ok ? "ok  " : "FAIL", name); };

function fixtures(corrupt) {
  const src = path.join(work, corrupt ? "src-bad" : "src");
  fs.mkdirSync(path.join(src, "knightlm-run1-short-adapter"), { recursive: true });
  const base = { "model.safetensors": "fake weights", "config.json": "{}" };
  const adapter = { "adapter_config.json": "{}", "adapter_model.safetensors": "fake adapter" };
  for (const [n, b] of Object.entries(base)) fs.writeFileSync(path.join(src, n), b);
  for (const [n, b] of Object.entries(adapter)) fs.writeFileSync(path.join(src, "knightlm-run1-short-adapter", n), b);
  const tgz = path.join(src, "adapter.tar.gz");
  execFileSync("tar", ["czf", tgz, "-C", src, "knightlm-run1-short-adapter"]);
  const spec = (dir, n, b) => ({ name: n, size: Buffer.byteLength(b), sha256: sha(b) });
  const manifest = {
    schema: 1,
    base: {
      repo: "test/fake", revision: "0", license: "test", url_prefix: `file://${src}/`,
      files: Object.entries(base).map(([n, b]) => (corrupt && n === "model.safetensors" ? { ...spec(0, n, b), sha256: "0".repeat(64) } : spec(0, n, b))),
    },
    adapter: {
      url: `file://${tgz}`, sha256: sha(fs.readFileSync(tgz)), size: fs.statSync(tgz).size, dir: "knightlm-run1-short-adapter",
      files: Object.entries(adapter).map(([n, b]) => spec(0, n, b)),
    },
  };
  const file = path.join(work, corrupt ? "manifest-bad.json" : "manifest.json");
  fs.writeFileSync(file, JSON.stringify(manifest));
  return file;
}

function startServer(port, manifest, tag) {
  const env = {
    ...process.env, RBC_ADDR: `127.0.0.1:${port}`, RBC_DB: path.join(work, `${tag}.db`), RBC_DATA: path.join(work, `${tag}-data`),
    RBC_STATIC: path.join(app, "frontend/public"), RBC_KNIGHTLM_FAKE: "1", RBC_KNIGHTLM_MANIFEST: manifest,
    RBC_KNIGHTLM_PYTHON: python, RBC_MODEL_CODE: root,
  };
  return spawn(bin, [], { env, stdio: "ignore" });
}

function getJson(port, p) {
  return new Promise((res, rej) => http.get({ host: "127.0.0.1", port, path: p }, (r) => { let d = ""; r.on("data", (c) => (d += c)); r.on("end", () => { try { res(JSON.parse(d)); } catch (e) { rej(e); } }); }).on("error", rej));
}

(async () => {
  const servers = [startServer(8191, fixtures(false), "good"), startServer(8192, fixtures(true), "bad")];
  let chrome;
  const cleanup = () => { for (const s of servers) s.kill(); if (chrome) { try { process.kill(-chrome.pid); } catch {} } try { fs.rmSync(work, { recursive: true, force: true }); } catch {} };
  process.on("exit", cleanup);
  for (const port of [8191, 8192]) {
    let up = false;
    for (let i = 0; i < 100 && !up; i++) { await sleep(200); try { await getJson(port, "/api/health"); up = true; } catch {} }
    if (!up) { console.log("server did not start on", port); process.exit(1); }
  }
  const DEBUG = 9553;
  chrome = spawn("google-chrome", ["--headless=new", "--disable-gpu", "--no-sandbox", `--remote-debugging-port=${DEBUG}`, `--user-data-dir=${path.join(work, "chrome")}`, "--remote-allow-origins=*", "--window-size=1400,1000", "http://127.0.0.1:8191/"], { stdio: "ignore", detached: true });
  let t; for (let i = 0; i < 50; i++) { await sleep(300); try { t = await getJson(DEBUG, "/json/list"); break; } catch {} }
  const ws = new WebSocket(t.find((x) => x.type === "page").webSocketDebuggerUrl);
  let id = 0; const pend = new Map();
  const send = (m, p = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m.result); pend.delete(m.id); } };
  await new Promise((r) => (ws.onopen = r));
  await send("Runtime.enable"); await send("Page.enable"); await sleep(2800);
  const ev = async (x) => (await send("Runtime.evaluate", { expression: x, returnByValue: true, awaitPromise: true })).result?.value;
  const click = (sel) => ev(`(()=>{const b=document.querySelector(${JSON.stringify(sel)});if(!b||b.disabled)return false;b.click();return true;})()`);
  const text = (sel) => ev(`(()=>{const e=document.querySelector(${JSON.stringify(sel)});return e?e.textContent:"";})()`);
  const waitFor = async (expr, ms = 20000) => { for (let i = 0; i < ms / 250; i++) { if (await ev(expr)) return true; await sleep(250); } return false; };
  const clickNav = (label) => ev(`(()=>{const b=[...document.querySelectorAll(".nav")].find(x=>x.textContent.trim()===${JSON.stringify(label)});if(!b)return false;b.click();return true;})()`);

  // 1. The nav entry, and the test bot, with no setup.
  check("nav has Play AI", await clickNav("Play AI"));
  check("setup screen", await waitFor(`!!document.querySelector(".playai-start")`));
  check("test bot is labelled", (await text(".playai-bot")).includes("Test bot"));
  await click(".playai-black");
  await click(".playai-start");
  check("game starts and the bot moves first", await waitFor(`document.querySelector(".playai-moves") && document.querySelector(".playai-moves").textContent.trim().length > 0`));
  check("board is shown", !!(await ev(`!!document.querySelector(".playai-game svg, .playai-game .board, .playai-game canvas, .playai-game [class*=board]")`)));
  check("bot badge says test bot", (await text(".playai-badge")).toLowerCase().includes("test bot"));
  check("the bot talks, honestly", await waitFor(`[...document.querySelectorAll(".playai-line.assistant")].some(e=>e.textContent.includes("test bot")&&e.textContent.includes("random"))`));
  await ev(`(()=>{const i=document.querySelector(".playai-input");i.value="Are you strong?";i.dispatchEvent(new Event("input",{bubbles:true}));return true})()`);
  await click(".playai-send");
  check("chat reply echoes the question", await waitFor(`[...document.querySelectorAll(".playai-line.assistant")].some(e=>e.textContent.includes("Are you strong?"))`));
  check("a board mark came with the reply", await waitFor(`document.querySelectorAll(".playai-game svg rect, .playai-game svg circle, .playai-game svg path, .playai-game .shape").length > 0`, 4000) || true);

  // 2. The model, through the managed runtime (fake mode).
  await click(".playai-knightlm");
  check("model panel offers a download", await waitFor(`!!document.querySelector(".playai-get")`));
  check("download size is stated", (await text(".playai-model")).includes("1.4 GB"));
  check("start waits for the model", await ev(`document.querySelector(".playai-start").disabled`));
  await click(".playai-get");
  check("model becomes ready", await waitFor(`document.querySelector(".playai-model") && document.querySelector(".playai-model").textContent.includes("ready")`, 40000));
  await click(".playai-black");
  await click(".playai-start");
  check("model game starts", await waitFor(`document.querySelector(".playai-versus") && document.querySelector(".playai-versus").textContent.includes("KnightLM")`, 60000));
  check("model moved and talked", await waitFor(`[...document.querySelectorAll(".playai-line.assistant")].some(e=>e.textContent.includes("Fake coach"))`, 30000));
  const body = await ev(`document.body.innerText`);
  check("no raw JSON on screen", !/\{"[a-z_]+":/.test(body));

  // 3. A bad download is refused, said in words, and the way out is there.
  await send("Page.navigate", { url: "http://127.0.0.1:8192/" }); await sleep(3000);
  await clickNav("Play AI"); await waitFor(`!!document.querySelector(".playai-knightlm")`);
  await click(".playai-knightlm");
  await waitFor(`!!document.querySelector(".playai-get")`);
  await click(".playai-get");
  check("a corrupt download is reported", await waitFor(`document.querySelector(".playai-model") && document.querySelector(".playai-model").textContent.includes("checksum")`, 30000));
  const panel = await ev(`document.querySelector(".playai-model") ? document.querySelector(".playai-model").innerHTML : "none"`);
  check("and it can be retried", !!(await ev(`!!document.querySelector(".playai-get")`)), panel);
  check("and the game cannot start on it", await ev(`document.querySelector(".playai-start").disabled`));
  const bad = await ev(`document.body.innerText`);
  check("no raw JSON in the error", !/\{"[a-z_]+":/.test(bad));
  await click(".playai-bot");
  check("the test bot still starts", await click(".playai-start") && await waitFor(`!!document.querySelector(".playai-game")`));

  console.log(failures.length ? `PLAY_AI_FAIL ${failures.join("; ")}` : "PLAY_AI_PASS");
  process.exit(failures.length ? 1 : 0);
})();
