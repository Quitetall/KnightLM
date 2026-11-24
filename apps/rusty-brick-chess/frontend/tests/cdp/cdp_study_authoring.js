// RBCUI-LIB-06. Exercise authoring from Library and continue the same study in Train.
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ command, evaluate, click, clickText, setField, destination, view }) => {
  await view("home");
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  await clickText('main button', 'Studies');
  await view("study");
  await click(".study-new");
  await waitFor(() => evaluate('!!document.querySelector(".study-title")'), "new study did not open");
  await setField(".study-title", "Parity study", "change");
  const record = () => evaluate('fetch("/api/studies").then(response => response.json()).then(rows => rows[0])');
  await waitFor(async () => (await record())?.title === "Parity study", "study title did not persist");
  const id = (await record()).id;
  const chapters = () => evaluate(`fetch("/api/studies/${id}/chapters").then(response => response.json())`);
  for (const [i,title] of ["Openings","Tactics"].entries()) {
    await setField(".chapter-title", title);
    await click(".chapter-add");
    await waitFor(async () => (await chapters()).length === i + 1, "chapter did not persist: " + title);
  }
  await waitFor(() => evaluate('document.querySelectorAll(".chapter-row").length === 2'), "chapters not displayed");
  const selectChapter = title => evaluate(`(() => {
    const row = [...document.querySelectorAll(".chapter-row")].find(row => row.querySelector(".chapter-name")?.textContent.trim() === ${JSON.stringify(title)});
    if (!row) return false;
    row.click();
    return true;
  })()`);
  if (!await selectChapter("Tactics")) throw new Error("Tactics missing");
  await setField(".study-description", "Shared authoring fixture", "change");
  await waitFor(async () => (await record())?.description_md === "Shared authoring fixture", "description did not save");
  await setField(".study-tags", "opening, tactics", "change");
  await waitFor(async () => JSON.parse((await record())?.tags_json || "[]").includes("tactics"), "tags did not save");
  await click(".chapter-row:first-child .chapter-duplicate");
  await waitFor(() => evaluate('document.querySelectorAll(".chapter-row").length === 3'), "chapter duplicate missing");
  const duplicate = (await chapters()).find(chapter => chapter.title.includes("copy"));
  if (!duplicate) throw new Error("could not identify duplicate");
  const before = (await chapters()).map(chapter => chapter.id).join(",");
  await click(".chapter-row:last-child .chapter-up");
  await waitFor(async () => (await chapters()).map(chapter => chapter.id).join(",") !== before, "chapter did not reorder");
  const deleteCopy = await evaluate(`(() => {
    const row = [...document.querySelectorAll(".chapter-row")].find(row => row.querySelector(".chapter-name")?.textContent.includes("copy"));
    if (!row) return false;
    row.querySelector(".chapter-delete").click();
    return true;
  })()`);
  if (!deleteCopy) throw new Error("duplicate delete control missing");
  await waitFor(async () => (await chapters()).length === 2 && !(await chapters()).some(chapter => chapter.id === duplicate.id), "wrong chapter deleted");
  await selectChapter("Tactics");
  await evaluate(`[...document.querySelectorAll(".chapter-row")].find(row => row.querySelector(".chapter-name")?.textContent.trim() === "Tactics").querySelector(".chapter-done input").click()`);
  await waitFor(() => evaluate('document.querySelector(".study-progress")?.textContent.trim() === "1/2"'), "study progress did not update");

  await destination("train");
  await click('[data-open-view="study"]');
  await view("study");
  await waitFor(() => evaluate('document.querySelector(".study-row")?.textContent.includes("Parity study")'), "Train did not show the same study");
  await click(".study-row");
  await waitFor(() => evaluate('document.querySelector(".chapter-row.selected .chapter-name")?.textContent.trim() === "Tactics" && document.querySelector(".chapter-row.selected input")?.checked'), "Train did not restore the same chapter and progress");
  const reopened = await record();
  if (reopened?.id !== id || reopened?.description_md !== "Shared authoring fixture") throw new Error("study identity or metadata changed across entry points");
  console.log("RBC_STUDY_AUTHORING_PASS");
}).catch(error => {
  console.error(error);
  console.log("RBC_STUDY_AUTHORING_FAIL");
  process.exitCode = 1;
});
