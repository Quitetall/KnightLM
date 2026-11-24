const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { createHash } = require("node:crypto");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, clickText, setField, destination, view }) => {
  const get = async path => {
    const response = await fetch(new URL("/api/" + path, base));
    assert.ok(response.ok);
    return response.json();
  };
  const wait = async (check, label) => {
    const deadline = Date.now() + 170000;
    while (Date.now() < deadline) {
      if (await check()) return;
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    throw new Error(label);
  };
  await view("home");
  await destination("settings");
  await view("settings");
  const metadata = await get("vision");
  assert.equal(metadata.installed, false);
  assert.match(metadata.sha256, /^[a-f0-9]{64}$/);
  await waitFor(() => evaluate(`document.querySelector(".settings-form")?.textContent.includes(${JSON.stringify(metadata.sha256)})
    && document.querySelector(".settings-form")?.textContent.includes(${JSON.stringify(metadata.license)})`), "vision metadata missing");
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const url = new URL(args[0].url || args[0], location.href);
      if (url.pathname === "/api/vision/download" && !window.failedVisionDownload) {
        window.failedVisionDownload = true;
        return new Response(JSON.stringify({error:"fixture download unavailable"}), {status:503,headers:{"content-type":"application/json"}});
      }
      return original(...args);
    };
  })()`);
  await clickText(".settings-form button", "Download model");
  await waitFor(() => evaluate('document.querySelector(".settings-form")?.textContent.includes("fixture download unavailable")'), "download failure feedback missing");
  assert.equal((await get("vision")).installed, false);
  await clickText(".settings-form button", "Download model");
  await wait(async () => (await get("vision")).installed, "real model download did not complete");
  const downloaded = readFileSync(metadata.path);
  assert.equal(downloaded.length, metadata.size);
  assert.equal(createHash("sha256").update(downloaded).digest("hex"), metadata.sha256);
  await waitFor(() => evaluate('[...document.querySelectorAll(".settings-form button")].some(button=>button.textContent.trim()==="Re-download model" && !button.disabled)'), "redownload action missing");
  await clickText(".settings-form button", "Re-download model");
  await wait(() => evaluate('[...document.querySelectorAll(".settings-form button")].some(button=>button.textContent.trim()==="Re-download model" && !button.disabled)'), "redownload did not complete");
  assert.equal(createHash("sha256").update(readFileSync(metadata.path)).digest("hex"), metadata.sha256);

  await destination("workbench");
  await click('[data-open-view="analysis"]');
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  const fen = () => evaluate('document.querySelector(".recorded-board .fen-line code")?.textContent');
  const initial = await fen();
  const clip = await evaluate(`(() => {
    const rect = document.querySelector(".recorded-board .board").getBoundingClientRect();
    return {x:rect.left,y:rect.top,width:rect.width,height:rect.height,scale:1};
  })()`);
  const { data: png } = await command("Page.captureScreenshot", {format:"png",clip,captureBeyondViewport:true});
  await clickText(".detailed-analysis button", "Edit position");
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const response = await original(...args);
      if (new URL(args[0].url || args[0], location.href).pathname === "/api/vision/recognize") {
        window.scanned = {ok:response.ok, body:await response.clone().json()};
      }
      return response;
    };
    const bytes = Uint8Array.from(atob(${JSON.stringify(png)}), c=>c.charCodeAt(0));
    const transfer = new DataTransfer();
    transfer.items.add(new File([bytes],"board.png",{type:"image/png"}));
    const input = document.querySelector(".editor input[type=file]");
    input.files = transfer.files;
    input.dispatchEvent(new Event("change",{bubbles:true}));
  })()`);
  await wait(() => evaluate("!!window.scanned"), "model inference did not finish");
  const scan = await evaluate("window.scanned");
  assert.ok(scan.ok, JSON.stringify(scan.body));
  assert.ok(scan.body.detections > 0);
  assert.equal(await fen(), initial, "scan applied without editable review");
  await waitFor(() => evaluate('document.querySelector(".editor")?.textContent.includes("pieces detected")'), "scan review feedback missing");

  // Correct the review to a known legal position through the palette.
  await clickText(".editor .palette button", "⌫");
  await evaluate('[...document.querySelectorAll("[data-edit-square]")].forEach(cell=>cell.click())');
  for (const [index, squares] of [[1,["e1"]],[3,["a1","h1"]],[7,["e8"]],[6,["d5"]],[12,["e5"]]]) {
    await click(`.editor .palette button:nth-child(${index})`);
    for (const square of squares) await click(`[data-edit-square="${square}"]`);
  }
  const placement = () => evaluate('[...document.querySelectorAll("[data-edit-square]")].map(cell=>cell.textContent).join("")');
  const original = await placement();
  for (const label of ["Flip files","Flip ranks","Rotate 180°"]) {
    await clickText(".editor button",label);
    assert.notEqual(await placement(), original, label + " did not transform pieces");
    await clickText(".editor button",label);
    assert.equal(await placement(), original);
  }
  await setField(".editor-fields label:nth-child(1) input","invalid");
  await clickText(".editor button","Use position");
  assert.equal(await fen(), initial, "invalid editor input replaced the board");
  assert.equal(await evaluate('!!document.querySelector(".editor")'), true, "invalid input closed the editor");
  assert.ok(await evaluate('document.querySelector(".editor > .muted")?.textContent.trim()'), "invalid input feedback missing");
  await setField(".editor-fields label:nth-child(1) input","w");
  await setField(".editor-fields label:nth-child(2) input","KQ");
  await setField(".editor-fields label:nth-child(3) input","e6");
  await setField(".editor-fields select","chess960","change");
  await setField(".editor-fields select","standard","change");
  await clickText(".editor button","Use position");
  await waitFor(async () => (await fen()) === "4k3/8/8/3Pp3/8/8/8/R3K2R w KQ e6 0 1", "reviewed position did not apply");
  await clickText(".detailed-analysis button","Edit position");
  await clickText(".detailed-analysis button","Close editor");
  assert.equal(await evaluate('!!document.querySelector(".editor")'), false);
  // Exercise the same installed model through Assistant attachments, including
  // keyboard palette selection and the context actually sent to the provider.
  await evaluate(`(async () => {
    const post = async (path, body) => {
      const response = await fetch(path,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(body)});
      if (!response.ok) throw new Error(await response.text());
      return response.json();
    };
    const workspace = await post("/api/workspaces",{title:"Mention workspace",kind:"analysis"});
    window.mentionBoard = await post("/api/boards", {parent_kind:"workspace",parent_id:workspace.id,title:"MentionBoard",kind:"analysis",start_fen:"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"});
    window.mentionStudy = await post("/api/studies", {title:"MentionStudy"});
  })()`);
  await destination("workbench");
  await click('[data-open-view="assistant"]');
  await view("assistant");
  await clickText("main button","New chat");
  await waitFor(() => evaluate('!!document.querySelector(".composer")'), "Assistant composer did not open");
  const enter = async () => {
    await evaluate('document.querySelector(".composer").focus()');
    await command("Input.dispatchKeyEvent",{type:"keyDown",key:"Enter",code:"Enter",text:"\r",windowsVirtualKeyCode:13});
    await command("Input.dispatchKeyEvent",{type:"keyUp",key:"Enter",code:"Enter",windowsVirtualKeyCode:13});
  };
  await setField(".composer", "/");
  await waitFor(() => evaluate('document.querySelectorAll(".composer-palette .item").length >= 3'), "slash palette missing");
  await evaluate('document.querySelector(".composer").focus()');
  await command("Input.dispatchKeyEvent",{type:"keyDown",key:"ArrowDown",code:"ArrowDown",windowsVirtualKeyCode:40});
  await waitFor(() => evaluate('document.querySelector(".composer-palette .selected")?.dataset.key === "/plan"'), "keyboard did not select second slash command");
  await enter();
  await waitFor(() => evaluate('document.querySelector(".composer").value.includes("plan")'), "keyboard slash selection did not fill draft");
  await setField(".composer", "@MentionBoard");
  await waitFor(() => evaluate('!!document.querySelector(".composer-palette.mention .item")'), "board mention missing");
  await click(".composer-palette.mention .item");
  await setField(".composer", "@MentionStudy");
  await waitFor(() => evaluate('!!document.querySelector(".composer-palette.mention .item")'), "study mention missing");
  await enter();
  assert.equal(await evaluate('document.querySelectorAll(".context-chip").length'),2);
  await setField(".composer", "4k3/8/8/3Pp3/8/8/8/R3K2R w KQ e6 0 1");
  await click(".attach-fen");
  await setField(".composer", '[Result "*"] 1. e4 e5 *');
  await click(".attach-pgn");
  await click(".context-chip[data-kind=fen] .chip-remove");
  assert.equal(await evaluate('document.querySelectorAll(".context-chip[data-kind=fen]").length'),0);
  await evaluate(`(() => {
    window.scanned = null;
    const bytes = Uint8Array.from(atob(${JSON.stringify(png)}), c=>c.charCodeAt(0));
    const transfer = new DataTransfer();
    transfer.items.add(new File([bytes],"board.png",{type:"image/png"}));
    const input = document.querySelector(".attach-image");
    input.files = transfer.files;
    input.dispatchEvent(new Event("change",{bubbles:true}));
  })()`);
  await wait(() => evaluate('!!document.querySelector(".context-chip[data-kind=position]")'), "image attachment did not become a position");
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const request = args[0] instanceof Request ? args[0].clone() : new Request(...args);
      if (new URL(request.url).pathname.endsWith("/send")) window.sentAttachments = await request.json();
      return original(...args);
    };
  })()`);
  await setField(".composer","Explain these attachments.");
  await click(".composer-send");
  await waitFor(() => evaluate('!!window.sentAttachments'), "attachment send missing");
  const sent = await evaluate("window.sentAttachments");
  const references = await evaluate('({board:window.mentionBoard.id,study:window.mentionStudy.id})');
  assert.ok(sent.message.includes(references.board) && sent.message.includes(references.study));
  assert.ok(sent.message.includes("pgn:") && sent.message.includes("position:"));
  console.log("RBC_VISION_EDITOR_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_VISION_EDITOR_FAIL");
  process.exitCode = 1;
});
