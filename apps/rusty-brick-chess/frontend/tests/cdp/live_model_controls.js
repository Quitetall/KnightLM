const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, evaluate, click, clickText, setField, destination, view }) => {
  const wait = async (check,label) => {
    const deadline=Date.now()+130000;
    while (Date.now()<deadline) {
      if (await check()) return;
      await new Promise(resolve=>setTimeout(resolve,500));
    }
    throw new Error(label);
  };
  const text=selector=>evaluate(`document.querySelector(${JSON.stringify(selector)})?.textContent.trim()`);
  const api=async path=>{
    const response=await fetch(new URL("/api/"+path,base));
    assert.ok(response.ok);
    return response.json();
  };
  await view("home");
  await destination("play");
  await view("play");
  await clickText(".play-head button","Load model");
  await wait(async ()=>(await text(".play-head [role=status]"))?.includes("is loaded"),"KnightLM did not load");
  await evaluate(`(() => {
    const original=window.fetch.bind(window);
    window.fetch=async (...args)=>{
      const request=args[0] instanceof Request ? args[0].clone() : new Request(...args);
      const response=await original(...args);
      if (new URL(request.url).pathname.endsWith("/model-move")) {
        window.liveMoveRequest=await request.json();
        window.liveMoveResponse=await response.clone().json();
      }
      return response;
    };
  })()`);
  await click(".play-start");
  await waitFor(()=>evaluate('document.querySelector(".match-status")?.textContent==="Your move"'),"KnightLM game did not open");
  const current=()=>evaluate('document.querySelector(".match-card.active")?.dataset.matchId');
  const id=await current();
  const game=()=>api("ai/matches/"+id);
  assert.equal((await game()).black.model,"knightlm-smolvlm-500m");
  assert.equal((await game()).black.provider,"local:LAMU");
  await evaluate(`document.querySelector('.active-match [data-square="e2"]').click()`);
  await evaluate(`document.querySelector('.active-match [data-square="e4"]').click()`);
  await wait(async ()=>(await game()).moves.length===2,"KnightLM did not reply to e4");
  assert.equal(await evaluate("window.liveMoveRequest.thinking"),false);
  assert.equal(await evaluate("window.liveMoveResponse.knightlm_vote"),null);
  assert.equal(await evaluate('document.querySelector(".active-match .board [data-square]").dataset.square'),"a8");
  await click(".match-takeback");
  await waitFor(async ()=>(await game()).moves.length===1,"takeback did not restore engine turn");
  await evaluate('document.querySelector(".play-head input[type=checkbox]").click()');
  await waitFor(()=>evaluate('!!document.querySelector(".play-head select:not(.play-opponent)")'),"sample controls missing");
  await setField(".play-head select:not(.play-opponent)","2","change");
  await setField(".play-head input[type=range][min='0.1']",0.1);
  await click(".match-model-move");
  await wait(async ()=>(await game()).moves.length===2 && await evaluate('window.liveMoveRequest?.knightlm_samples===2 && !!window.liveMoveResponse.knightlm_vote'),"sampled reply did not finish");
  assert.ok(Math.abs(await evaluate("window.liveMoveRequest.knightlm_temperature") - 0.1) < 1e-6);
  const vote=await evaluate("window.liveMoveResponse.knightlm_vote");
  assert.equal(vote.samples,2);
  assert.ok(vote.winning_votes>0 && vote.legal_samples>=vote.winning_votes && vote.legal_samples<=2);
  const direct=vote.direct_move;
  await waitFor(async ()=>(await text(".match-inspector")).includes(direct),"vote summary or direct move missing");
  await click(".match-takeback");
  await waitFor(async ()=>(await game()).moves.length===1,"second takeback did not restore engine turn");
  await setField(".play-head select:not(.play-opponent)","9","change");
  await setField(".play-head input[type=range][min='0.1']",1.5);
  await click(".match-model-move");
  await wait(async ()=>(await game()).moves.length===2 && await evaluate('window.liveMoveRequest?.knightlm_samples===9'),"longer-thinking reply did not complete");
  assert.equal(await evaluate("window.liveMoveRequest.thinking"),true);
  assert.equal(await evaluate("window.liveMoveRequest.knightlm_temperature"),1.5);

  await waitFor(()=>evaluate('!document.querySelector(".play-start").disabled'),"model move did not leave the busy state");
  await setField(".play-opponent","0","change");
  assert.equal(await evaluate('document.querySelector(".play-head input[type=checkbox]").disabled'),true);
  await click(".play-start");
  await waitFor(async ()=>await current()!==id,"default endpoint game did not start");
  assert.equal((await api("ai/matches/"+await current())).black.model,"assistant");
  await evaluate('document.querySelector(".play-more").open = true');
  await click(".play-scan-models");
  await waitFor(()=>evaluate('[...document.querySelectorAll(".play-opponent option")].some(option=>option.textContent.includes("Ollama"))'),"local model discovery missing");
  const choice=await evaluate('[...document.querySelectorAll(".play-opponent option")].find(option=>option.textContent.includes("Ollama")).value');
  await setField(".play-opponent",choice,"change");
  await click(".play-start");
  await waitFor(async ()=>(await api("ai/matches/"+await current())).black.provider==="local:Ollama","discovered model seat did not start");
  await click(".play-refresh");
  await waitFor(()=>evaluate('document.querySelectorAll(".match-card").length>=3'),"game refresh lost matches");
  await setField(".play-opponent","1","change");
  await clickText(".play-head button","Unload model");
  await wait(async ()=>(await text(".play-head [role=status]"))?.includes("is unloaded"),"private KnightLM did not unload");
  console.log("RBC_LIVE_MODEL_CONTROLS_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_LIVE_MODEL_CONTROLS_FAIL");
  process.exitCode=1;
});
