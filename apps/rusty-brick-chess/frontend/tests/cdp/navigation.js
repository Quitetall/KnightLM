// Follow visible hub controls using stable, language-independent destinations.
async function openView(evaluate, group, section) {
  const click = selector => evaluate(`(() => {
    const button = document.querySelector(${JSON.stringify(selector)});
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!await click(`[data-destination="${group}"]`)) return false;
  if (!section) return true;
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline) {
    if (await click(`[data-open-view="${section}"]`)) return true;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  return false;
}

module.exports = { openView };
