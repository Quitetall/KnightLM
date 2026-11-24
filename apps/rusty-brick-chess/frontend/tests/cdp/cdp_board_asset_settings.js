// RBCUI-CFG-03. Persist board colors, artwork, labels and animation.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const settings = () => evaluate('fetch("/api/settings").then(response => response.json())');
  const openSettings = async () => { await destination("settings"); await view("settings"); };
  const checkbox = async (label, checked) => evaluate(`(() => {
    const row = [...document.querySelectorAll(".settings-form label")].find(row => row.textContent.trim() === ${JSON.stringify(label)});
    const input = row?.querySelector('input[type=checkbox]');
    if (!input) throw new Error("checkbox missing");
    if (input.checked !== ${checked}) input.click();
  })()`);
  const upload = async (name, complete) => {
    await setField(".piece-upload input[type=text]", name);
    await evaluate(`(() => {
      const transfer = new DataTransfer();
      const codes = ${complete} ? "KQRBNPkqrbnp" : "K";
      for (const code of codes) transfer.items.add(new File([
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 45 45"><circle cx="22" cy="22" r="14" fill="#714a24"/></svg>',
      ], code + ".svg", {type:"image/svg+xml"}));
      const input = document.querySelector(".piece-upload input[type=file]");
      input.files = transfer.files;
      input.dispatchEvent(new Event("change",{bubbles:true}));
    })()`);
  };
  const inventory = name => evaluate(`[...document.querySelectorAll(".custom-set")].some(set => set.textContent.trim().startsWith(${JSON.stringify(name + " ·")}))`);
  const remove = name => evaluate(`(() => {
    const row = [...document.querySelectorAll(".custom-set")].find(row => row.textContent.trim().startsWith(${JSON.stringify(name + " ·")}));
    if (!row) return false;
    row.querySelector("button").click();
    return true;
  })()`);

  await openSettings();
  await upload("invalid-set", false);
  await waitFor(() => evaluate('document.querySelector(".piece-upload .muted")?.textContent.toLowerCase().includes("missing")'), "incomplete piece set did not report an error");
  if (await inventory("invalid-set")) throw new Error("invalid set was stored");
  for (const name of ["parity", "parity-long"]) {
    await upload(name, true);
    await waitFor(async () => await inventory(name) && (await settings()).pieces === "custom:" + name, "uploaded set not selected: " + name);
  }

  await setField('.settings-form input[type=color]:first-of-type', "#d8c091");
  await setField('.settings-form label:has(input[type=color]):nth-last-child(2) input[type=color]', "#365548");
  await checkbox("Coordinates", false);
  await checkbox("Animate the last move", true);
  await setField('.settings-form input[type=range][max="400"]', "180");
  await clickText(".settings-form button", "Save");
  await waitFor(async () => {
    const saved = await settings();
    return saved.theme === "custom" && saved.board_light === "#d8c091" && saved.board_dark === "#365548"
      && saved.show_coords === false && saved.animate_last_move === true && saved.piece_animation_ms === 180;
  }, "board preferences did not persist");
  await command("Page.reload");
  await waitFor(() => evaluate('performance.getEntriesByType("navigation")[0]?.type === "reload" && !!document.querySelector(".settings-form")'), "settings did not reload");
  await setField(".ws-new", "analysis", "change");
  await view("analysis");
  try {
  await waitFor(() => evaluate(`document.querySelector(".recorded-board .board.custom.animate-last")?.style.getPropertyValue("--board-light").trim() === "#d8c091"
    && !document.querySelector(".recorded-board .coord")
    && [...document.querySelectorAll(".recorded-board img.piece-img")].some(piece => piece.src.includes("/api/pieces/parity-long/") && piece.complete && piece.naturalWidth > 0)`), "rendered board did not restore custom preferences");
  } catch (error) {
    console.log(JSON.stringify(await evaluate(`(() => {
      const board = document.querySelector(".recorded-board .board");
      const piece = board?.querySelector("img.piece-img");
      return { class:board?.className, style:board?.getAttribute("style"), coordinates:board?.querySelectorAll(".coord").length,
        image:piece?.src, width:piece?.naturalWidth, complete:piece?.complete };
    })()`)));
    throw error;
  }

  await openSettings();
  if (!await remove("parity")) throw new Error("prefix set missing");
  await waitFor(async () => !await inventory("parity") && (await settings()).pieces === "custom:parity-long", "removing a similarly named set changed the active artwork");
  if (!await remove("parity-long")) throw new Error("active set missing");
  await waitFor(async () => !await inventory("parity-long") && (await settings()).pieces === "cburnett", "active set deletion did not persist fallback");
  await setField(".ws-new", "analysis", "change");
  await view("analysis");
  await waitFor(() => evaluate('!!document.querySelector(".recorded-board .board svg") && !document.querySelector(".recorded-board img.piece-img")'), "fallback artwork did not render");
  console.log("RBC_BOARD_ASSET_SETTINGS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_BOARD_ASSET_SETTINGS_FAIL");
  process.exitCode = 1;
});
