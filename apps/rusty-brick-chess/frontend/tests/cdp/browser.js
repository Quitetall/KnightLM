const { spawn } = require("node:child_process");
const { once } = require("node:events");
const { mkdtempSync, readFileSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");

const base = process.env.RBC_CDP_URL || "http://127.0.0.1:8090/";
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

async function withBrowser(test) {
  const profile = mkdtempSync(join(tmpdir(), "rbc-cdp-"));
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
    const clickText = async (selector, text) => {
      const found = await evaluate(`(() => {
        const button = [...document.querySelectorAll(${JSON.stringify(selector)})].find(item => item.textContent.trim() === ${JSON.stringify(text)});
        if (!button || button.disabled) return false;
        button.click();
        return true;
      })()`);
      if (!found) throw new Error("missing enabled control: " + text);
    };
    const setField = (selector, value, event = "input") => evaluate(`(() => {
      const field = document.querySelector(${JSON.stringify(selector)});
      if (!field) throw new Error("field missing");
      field.value = ${JSON.stringify(value)};
      field.dispatchEvent(new Event(${JSON.stringify(event)}, {bubbles:true}));
    })()`);
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
    await test({ base, command, evaluate, click, clickText, setField, view, destination, screenshot });
  } finally {
    ws?.close();
    if (browser.exitCode === null && browser.signalCode === null) {
      const exited = once(browser, "exit");
      if (browser.kill()) await exited;
    }
    rmSync(profile, { recursive: true, force: true });
  }
}

module.exports = { withBrowser, waitFor };
