// RBCUI-AN-03. Verify node-scoped annotations using actual board gestures.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  const game = await evaluate(`(async () => {
    const pgn = '[White "Annotated"]\\n[Black "Opponent"]\\n[Result "*"]\\n\\n1. e4 (1. d4 d5) e5 *';
    const response = await fetch("/api/games", {
      method:"POST", headers:{"content-type":"application/json"}, body:JSON.stringify({pgn}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  if (!game) throw new Error("could not seed annotation fixture");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await click(".topbar button");
  await waitFor(() => evaluate(`!!document.querySelector('[data-game-id="${game}"]')`), "annotation game missing");
  await click(`[data-game-id="${game}"]`);
  await view("analysis");
  await click('[data-analysis-view="detailed"]');

  const selectNode = async san => {
    await waitFor(() => evaluate(`(() => {
      const node = [...document.querySelectorAll(".tree .tnode")].find(item => item.textContent.includes(${JSON.stringify(san)}));
      if (!node) return false;
      node.click();
      return true;
    })()`), "tree move missing: " + san);
  };
  const gesture = async (from, to, modifiers) => {
    const center = name => evaluate(`(() => {
      const box = document.querySelector('.recorded-board [data-square="${name}"]').getBoundingClientRect();
      return {x:box.left + box.width/2, y:box.top + box.height/2};
    })()`);
    const source = await center(from);
    const target = await center(to);
    await command("Input.dispatchMouseEvent", {type:"mousePressed", ...source, button:"right", buttons:2, clickCount:1, modifiers});
    if (from !== to) await command("Input.dispatchMouseEvent", {type:"mouseMoved", ...target, buttons:2, modifiers});
    await command("Input.dispatchMouseEvent", {type:"mouseReleased", ...target, button:"right", buttons:0, clickCount:1, modifiers});
  };
  const annotationRows = () => evaluate(`fetch("/api/games/${game}/annotations").then(response => response.json())`);
  const comment = () => evaluate('document.querySelector(".ann-editor input")?.value');
  await selectNode("e4");
  await gesture("e2", "e4", 8);
  await gesture("e4", "e4", 1);
  await gesture("d2", "d4", 2);
  await setField(".ann-editor input", "Main line note");
  await setField(".ann-editor select", "1", "change");
  await clickText(".ann-editor button", "Save");
  await waitFor(async () => (await annotationRows()).some(row => row.comment === "Main line note"), "main annotation was not saved");
  const primary = (await annotationRows()).find(row => row.comment === "Main line note");
  const shapes = JSON.parse(primary.shapes);
  if (primary.nag !== 1 || shapes.length !== 3 ||
      !shapes.some(shape => shape.kind === "arrow" && shape.color === "red" && shape.from === "e2" && shape.to === "e4") ||
      !shapes.some(shape => shape.kind === "circle" && shape.color === "blue" && shape.square === "e4") ||
      !shapes.some(shape => shape.kind === "arrow" && shape.color === "yellow" && shape.from === "d2")) {
    throw new Error("wrong saved NAG or board shapes: " + JSON.stringify(primary));
  }
  await selectNode("d4");
  await waitFor(async () => await comment() === "", "variation inherited the main-line comment");
  await setField(".ann-editor input", "Variation note");
  await setField(".ann-editor select", "5", "change");
  await clickText(".ann-editor button", "Save");
  await waitFor(async () => (await annotationRows()).some(row => row.comment === "Variation note"), "variation annotation not saved");
  const branch = (await annotationRows()).find(row => row.comment === "Variation note");
  if (branch.path === primary.path || branch.nag !== 5) throw new Error("variation annotation overwrote the main line");

  await command("Page.reload");
  await view("analysis");
  await click('[data-analysis-view="detailed"]');
  await selectNode("e4");
  await waitFor(async () => await comment() === "Main line note" && await evaluate('document.querySelector(".recorded-board .shapes circle") !== null'), "main comment and shapes did not restore");
  await selectNode("d4");
  await waitFor(async () => await comment() === "Variation note" && !await evaluate('!!document.querySelector(".recorded-board .shapes circle")'), "variation comment or shape isolation lost after reload");
  const exported = await evaluate(`fetch("/api/games/${game}/pgn").then(response => response.text())`);
  if (!["Main line note", "Variation note", "$1", "$5", "[%cal Re2e4,Yd2d4]", "[%csl Be4]"].every(part => exported.includes(part))) {
    throw new Error("PGN export omitted saved annotations");
  }
  const reimport = await evaluate(`(async () => {
    const response = await fetch("/api/games", {
      method:"POST",headers:{"content-type":"application/json"},
      body:JSON.stringify({pgn:${JSON.stringify(exported)}}),
    });
    return (await response.json()).imported?.[0];
  })()`);
  if (!reimport) throw new Error("annotated PGN could not be reimported");
  const roundTrip = await evaluate(`fetch("/api/games/${reimport}/pgn").then(response => response.text())`);
  if (roundTrip !== exported) throw new Error("annotated PGN changed on reimport");
  console.log("RBC_ANALYSIS_ANNOTATIONS_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_ANALYSIS_ANNOTATIONS_FAIL");
  process.exitCode = 1;
});
