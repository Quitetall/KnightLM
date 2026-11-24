// RBCUI-CFG-04. Sound enable, theme, volume, custom upload, failure feedback
// and move/capture/game-ending playback.
//
// The page's real HTMLAudioElement.play is wrapped to record every call and
// its outcome. A playback counts only if the promise resolves and the media
// clock advances, so a host with no audio service fails here instead of
// passing on a stub. Run with PULSE_SINK pointing at a null sink so it is
// silent on a shared workstation.
const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, clickText, setField, destination, view }) => {
  {
    await view("home");
    await evaluate(`(() => {
      window.soundLog = [];
      const original = HTMLMediaElement.prototype.play;
      HTMLMediaElement.prototype.play = function () {
        const entry = {src: new URL(this.src).pathname, volume: this.volume, ok: null, advanced: false};
        window.soundLog.push(entry);
        const result = original.call(this);
        Promise.resolve(result).then(() => {
          entry.ok = true;
          const started = Date.now();
          const poll = () => {
            if (this.currentTime > 0) entry.advanced = true;
            else if (Date.now() - started < 3000) setTimeout(poll, 50);
          };
          poll();
        }, error => { entry.ok = false; entry.error = String(error); });
        return result;
      };
    })()`);
    const plays = () => evaluate("JSON.parse(JSON.stringify(window.soundLog))");
    const clear = () => evaluate("window.soundLog.length = 0");

    await destination("settings");
    await view("settings");
    const toggle = '.settings-row:has(input[type=range][max="100"]) .checkline input[type=checkbox]';
    const theme = '.settings-form select:has(option[value="piano"])';
    const volume = '.settings-row input[type=range][max="100"]';
    for (const control of [toggle, theme, volume]) {
      assert.ok(await evaluate(`!!document.querySelector(${JSON.stringify(control)})`), "missing sound control " + control);
    }
    assert.equal(await evaluate(`document.querySelector(${JSON.stringify(toggle)}).checked`), true);
    const themes = await evaluate(`[...document.querySelector(${JSON.stringify(theme)}).options].map(o => o.value)`);
    for (const name of ["sfx", "piano", "nes", "futuristic", "custom"]) assert.ok(themes.includes(name), "theme missing: " + name);

    // Board helpers (same pointer path as cdp_board_gestures).
    await destination("workbench");
    await click('[data-open-view="analysis"]');
    await view("analysis");
    await click('[data-analysis-view="detailed"]');
    const board = ".recorded-board";
    const fen = () => evaluate(`document.querySelector("${board} .fen-line code")?.textContent`);
    const point = name => evaluate(`(() => {
      const cell = document.querySelector(${JSON.stringify(board + ' .board [data-square="' + name + '"]')});
      const rect = cell.getBoundingClientRect();
      return {x: rect.left + rect.width / 2, y: rect.top + rect.height / 2};
    })()`);
    const tap = async name => {
      const xy = await point(name);
      await command("Input.dispatchMouseEvent", {type: "mousePressed", ...xy, button: "left", buttons: 1, clickCount: 1});
      await command("Input.dispatchMouseEvent", {type: "mouseReleased", ...xy, button: "left", buttons: 0, clickCount: 1});
    };
    const loadFen = async value => {
      await setField(board + " .load input", value);
      await click(board + " .load button");
      await waitFor(async () => (await fen()) === value, "FEN did not load");
    };
    const settle = () => new Promise(done => setTimeout(done, 400));
    const play = async (from, to) => {
      await clear();
      await tap(from);
      await tap(to);
      await waitFor(async () => (await plays()).some(p => /\/(Move|Capture|Check|Checkmate|Victory|Draw)\.ogg$/.test(p.src)), `no move sound for ${from}${to}`);
      await waitFor(async () => (await plays()).filter(p => !/\/Select\.ogg$/.test(p.src)).every(p => p.ok !== null), "play() outcome unknown");
      await settle();
      return plays();
    };
    const real = list => list.filter(p => !/\/Select\.ogg$/.test(p.src));
    const assertAudible = (list, pattern, volume) => {
      const hit = real(list).find(p => pattern.test(p.src));
      assert.ok(hit, `expected ${pattern} in ${JSON.stringify(list)}`);
      assert.equal(hit.ok, true, "play() was rejected: " + (hit.error || "pending"));
      assert.ok(hit.advanced, "media clock did not advance: " + hit.src);
      assert.ok(Math.abs(hit.volume - volume) < 0.001, `volume ${hit.volume} != ${volume}`);
    };
    const settings = async () => {
      await destination("settings");
      await view("settings");
    };
    const analysis = async () => {
      await destination("workbench");
      await click('[data-open-view="analysis"]');
      await view("analysis");
      await click('[data-analysis-view="detailed"]');
    };

    // Bundled theme, audible: move and capture.
    let list = await play("e2", "e4");
    assertAudible(list, /\/sounds\/sfx\/Move\.ogg$/, 0.7);
    await loadFen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
    list = await play("e4", "d5");
    assertAudible(list, /\/sounds\/sfx\/Capture\.ogg$/, 0.7);
    // Game ending: fool's-mate position, Black mates with Qh4.
    await loadFen("rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq - 0 2");
    list = await play("d8", "h4");
    assertAudible(list, /\/sounds\/sfx\/(Checkmate|Victory)\.ogg$/, 0.7);

    // Quiet path: volume 0 plays nothing; theme and volume change are honoured.
    await settings();
    await setField(volume, "30", "input");
    await setField(theme, "piano", "change");
    await analysis();
    await loadFen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    list = await play("e2", "e4");
    assertAudible(list, /\/sounds\/piano\/Move\.ogg$/, 0.3);
    await settings();
    await setField(volume, "0", "input");
    await analysis();
    await loadFen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    await clear();
    await tap("d2");
    await tap("d4");
    await waitFor(async () => (await fen()).startsWith("rnbqkbnr/pppppppp/8/8/3P4/"), "move did not apply");
    await settle();
    assert.deepEqual(await plays(), [], "volume 0 still played audio");

    // Disabled: nothing plays even at full volume.
    await settings();
    await setField(volume, "100", "input");
    await click(toggle);
    assert.equal(await evaluate(`document.querySelector(${JSON.stringify(toggle)}).checked`), false);
    await analysis();
    await loadFen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    await clear();
    await tap("e2");
    await tap("e4");
    await waitFor(async () => (await fen()).startsWith("rnbqkbnr/pppppppp/8/8/4P3/"), "move did not apply");
    await settle();
    assert.deepEqual(await plays(), [], "disabled sound still played audio");

    // Custom upload: a real .ogg named after the event becomes the custom theme.
    const bundled = Buffer.from(await (await fetch(new URL("sounds/sfx/Move.ogg", base))).arrayBuffer());
    assert.ok(bundled.length > 1000 && bundled.subarray(0, 4).toString() === "OggS", "bundled Move.ogg is not Ogg audio");
    await settings();
    await click(toggle);
    const upload = (name, bytes) => evaluate(`(() => {
      const input = document.querySelector('input[type=file][accept*="ogg"]');
      if (!input) throw new Error("custom sound input missing");
      const data = Uint8Array.from(atob(${JSON.stringify(bytes.toString("base64"))}), c => c.charCodeAt(0));
      const transfer = new DataTransfer();
      transfer.items.add(new File([data], ${JSON.stringify(name)}, {type: "audio/ogg"}));
      input.files = transfer.files;
      input.dispatchEvent(new Event("change", {bubbles: true}));
    })()`);
    await upload("Move.ogg", bundled);
    await waitFor(() => evaluate('document.querySelector("label:has(input[type=file][accept*=ogg]) + .muted")?.textContent.trim().length > 0'), "no upload feedback");
    await waitFor(() => evaluate(`document.querySelector(${JSON.stringify(theme)}).value === "custom"`), "custom theme not selected");
    const served = await evaluate('fetch("/api/sounds/custom/Move.ogg").then(r => r.ok ? r.arrayBuffer().then(b => b.byteLength) : -1)');
    assert.ok(served > 100, "custom sound not served: " + served);
    await analysis();
    await loadFen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    list = await play("e2", "e4");
    assertAudible(list, /\/api\/sounds\/custom\/Move\.ogg$/, 1);

    // Failure feedback: an unsafe file name is refused and the message says so.
    await settings();
    const before = await evaluate('document.querySelector("label:has(input[type=file][accept*=ogg]) + .muted")?.textContent');
    await upload("bad name.ogg", Buffer.from("not audio"));
    await waitFor(() => evaluate(`document.querySelector("label:has(input[type=file][accept*=ogg]) + .muted")?.textContent !== ${JSON.stringify(before)}`), "no failure feedback");
    const message = await evaluate('document.querySelector("label:has(input[type=file][accept*=ogg]) + .muted")?.textContent');
    assert.ok(/invalid|error|fail|name/i.test(message), "unhelpful failure message: " + message);
    assert.equal((await evaluate('fetch("/api/sounds/custom/bad name").then(r => r.status)')) >= 400, true);

    // Every bundled theme must ship real audio for every event, not a stub.
    const broken = [];
    for (const name of ["sfx", "piano", "nes", "futuristic"]) {
      for (const event of ["Move", "Capture", "Check", "Checkmate", "LowTime", "Error", "Victory", "Defeat", "Draw", "Select", "Confirmation"]) {
        const bytes = Buffer.from(await (await fetch(new URL(`sounds/${name}/${event}.ogg`, base))).arrayBuffer());
        if (bytes.length < 1000 || bytes.subarray(0, 4).toString() !== "OggS") broken.push(`${name}/${event}.ogg (${bytes.length} bytes)`);
      }
    }
    assert.deepEqual(broken, [], "bundled sound files are not Ogg audio");

    console.log("RBC_SOUND_PASS");
  }
}).catch(error => {
  console.error(error);
  console.log("RBC_SOUND_FAIL");
  process.exitCode = 1;
});
