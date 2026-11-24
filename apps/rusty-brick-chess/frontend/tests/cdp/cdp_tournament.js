const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

// RBCUI-PLAY-10. Start real matches through the tournament UI, then play and
// export every format against the isolated runner's deterministic provider.
withBrowser(async ({ base, evaluate, click, setField, view, destination }) => {
  const fixture = process.env.RBC_CDP_TOURNAMENT_FIXTURE === "1";
  const get = async path => {
    const response = await fetch(new URL("/api/ai/" + path, base));
    assert.ok(response.ok, path + " returned " + response.status);
    return response.json();
  };
  const rows = id => get("matches").then(matches => matches.filter(row => row.id.startsWith(id + "-r")));
  const log = () => evaluate('document.querySelector(".tournament-log")?.textContent || ""');
  const details = id => get("tournaments/" + id);
  const competitors = ["Alpha", "Beta", "Gamma", "Delta"].map(name => "llm:fixture:" + name + ":v1");

  await view("home");
  await destination("workbench");
  await click('[data-open-view="tournaments"]');
  await view("tournaments");
  await evaluate(`(() => {
    const makeUrl = URL.createObjectURL;
    URL.createObjectURL = function(blob) {
      window.tournamentPgn = blob.text();
      return makeUrl.call(this, blob);
    };
    HTMLAnchorElement.prototype.click = function() { window.tournamentFile = this.download; };
  })()`);

  const action = async name => {
    await waitFor(() => evaluate(`!document.querySelector(".tournament-${name}").disabled`), name + " stayed disabled");
    const before = await log();
    await click(".tournament-" + name);
    await waitFor(async () => (await log()) !== before, name + " did not record an event");
  };
  let previousName;
  for (const [format, entrants, total] of [
    ["double-round-robin", competitors, 12],
    ...(fixture ? [["swiss", competitors.slice(0, 2), 3], ["knockout", competitors, 3]] : []),
  ]) {
    const name = "Fixture " + format;
    await setField(".tournament-name", name);
    await setField(".tournament-format", format, "change");
    await setField(".tournament-competitors", entrants.join(", "));
    await click(".tournament-create");
    await waitFor(() => evaluate(`document.querySelector(".tournament-row.active")?.textContent.includes(${JSON.stringify(name)})`),
      "created tournament did not become active");
    const id = (await get("tournaments")).find(row => row.name === name).id;
    if (previousName) {
      await evaluate(`[...document.querySelectorAll(".tournament-row")].find(button=>button.textContent.includes(${JSON.stringify(previousName)})).click()`);
      await waitFor(() => evaluate(`document.querySelector(".tournament-row.active")?.textContent.includes(${JSON.stringify(previousName)})`), "could not select earlier tournament");
      await evaluate(`[...document.querySelectorAll(".tournament-row")].find(button=>button.textContent.includes(${JSON.stringify(name)})).click()`);
    }
    previousName = name;
    assert.deepEqual(await rows(id), []);
    await action("step");
    const started = await rows(id);
    assert.equal(started.length, Math.floor(entrants.length / 2), "Step did not create matches");
    for (const row of started) {
      const game = await get("matches/" + row.id);
      assert.equal(game.moves.length, 0);
      assert.ok(game.legal_moves.includes("e4"), "match lacks a legal starting position");
      assert.equal(game.white.provider, "fixture");
      assert.equal(game.black.provider, "fixture");
      assert.ok(entrants.includes("llm:fixture:" + game.white.model + ":v1"));
      assert.ok(entrants.includes("llm:fixture:" + game.black.model + ":v1"));
    }
    await waitFor(() => evaluate(`document.querySelectorAll(".pairing").length === ${started.length}`), "pairings did not render");
    await action("pause");
    assert.equal((await get("tournaments")).find(row => row.id === id).status, "Paused");
    await action("step");
    assert.deepEqual(await rows(id), started, "paused Step modified the matches");
    await action("resume");
    assert.match(await log(), /resume: running/);
    if (fixture) {
      let detail = await details(id);
      for (let count = 1; detail.tournament.status !== "Finished" && count < 35; count++) {
        await action("step");
        detail = await details(id);
      }
      assert.equal(detail.tournament.status, "Finished", format + " never finished");
      assert.equal((await rows(id)).length, total, "duplicate or missing matches");
      const games = detail.tournament.schedule.rounds.flatMap(round => round.games);
      assert.equal(games.length, total);
      const scores = new Map(entrants.map(entrant => [entrant, {wins:0, faced:[]}]));
      for (const game of games) {
        assert.equal(game.outcome, "Black");
        scores.get(game.black).wins++;
        scores.get(game.black).faced.push(game.white);
        scores.get(game.white).faced.push(game.black);
      }
      for (const standing of detail.standings) {
        const expected = scores.get(standing.competitor);
        assert.equal(standing.points, expected.wins);
        assert.equal(standing.wins, expected.wins);
        assert.equal(standing.buchholz, [...expected.faced].reduce((n, player) => n + scores.get(player).wins, 0));
      }
      await waitFor(() => evaluate('document.querySelector(".pairings")?.textContent.includes("0-1")'), "result missing from pairing");
      assert.match(await log(), /completed [12]/);
    }
    await evaluate('window.tournamentFile = null');
    await click(".tournament-export");
    await waitFor(() => evaluate('window.tournamentFile === "match.pgn"'), "PGN download missing");
    const pgn = await evaluate("window.tournamentPgn");
    assert.ok(pgn.includes(`[Event "${name}"]`));
    if (fixture) assert.equal((pgn.match(/1\. f3 e5 2\. g4 Qh4# 0-1/g) || []).length, total);
    console.log(JSON.stringify({ format, started: started.map(row => row.id), completed: fixture ? total : null }));
  }
  console.log("AI_TOURNAMENT_PASS");
}).catch(error => {
  console.error(error);
  console.log("AI_TOURNAMENT_FAIL");
  process.exitCode = 1;
});
