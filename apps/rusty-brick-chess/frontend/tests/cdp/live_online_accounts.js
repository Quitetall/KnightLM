const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

// Optional authenticated path. The token comes only from LICHESS_TOKEN (run
// under `secrets run --`). It is typed into the UI field and never printed:
// every console write and thrown message is scrubbed of it first.
const TOKEN=(process.env.LICHESS_TOKEN||"").trim();
if (TOKEN) {
  const scrub=value=>String(value).split(TOKEN).join("[token]");
  for (const name of ["log","error","warn"]) {
    const original=console[name].bind(console);
    console[name]=(...args)=>original(...args.map(arg=>arg instanceof Error ? scrub(arg.stack||arg.message) : scrub(typeof arg==="string" ? arg : JSON.stringify(arg))));
  }
}

withBrowser(async ({ base, command, evaluate, click, clickText, setField, destination, view }) => {
  const api = async path => {
    const response=await fetch(new URL("/api/"+path,base));
    assert.ok(response.ok,path+" returned "+response.status);
    return response.json();
  };
  const wait=async (check,label)=>{
    const deadline=Date.now()+90000;
    while (Date.now()<deadline) {
      if (await check()) return;
      await new Promise(resolve=>setTimeout(resolve,400));
    }
    throw new Error(label);
  };
  const accounts=()=>api("accounts");
  const home=async ()=>{
    await destination("home");
    await view("home");
    await evaluate('document.querySelector(".home details").open = true');
  };
  await view("home");
  const observeOnline=()=>evaluate(`(() => {
    if (window.onlineObserverInstalled) return;
    window.onlineObserverInstalled=true;
    const original=window.fetch.bind(window);
    window.fetch=async (...args)=>{
      const response=await original(...args);
      const path=new URL(args[0].url || args[0],location.href).pathname;
      if (path.includes("/api/import/") || path === "/api/accounts/import") {
        window.onlineResult={path,ok:response.ok,body:await response.clone().json()};
      }
      return response;
    };
  })()`);
  await observeOnline();
  await destination("workbench");
  await click('[data-open-view="database"]');
  await view("database");
  const online='.dbs:has(select option[value="chesscom"])';
  await setField(online+' input[placeholder="Username"]',"bad user!");
  await clickText(online+" button","Import online");
  await waitFor(()=>evaluate('window.onlineResult?.ok === false'),"invalid username did not fail");
  await setField(online+" select","lichess","change");
  await setField(online+' input[placeholder="Username"]',"thibault");
  await setField(online+' input[type=number]',"1");
  await evaluate("window.onlineResult = null");
  await clickText(online+" button","Import online");
  await wait(()=>evaluate('window.onlineResult?.ok === true'),"anonymous Lichess import failed");
  assert.equal(await evaluate("window.onlineResult.body.imported"),1);
  await waitFor(()=>evaluate('document.querySelector("[data-game-id]")?.textContent.toLowerCase().includes("thibault")'),"anonymous game not listed");

  for (const [index,provider,user] of [[1,"lichess","thibault"],[2,"chesscom","hikaru"]]) {
    await home();
    await observeOnline();
    const card=`.account-card:nth-child(${index})`;
    const field=provider==="lichess" ? "Lichess username" : "Chess.com username";
    if (provider==="lichess") assert.equal(await evaluate(`document.querySelector('${card} input[type=password]').value`),"");
    await setField(`${card} input[placeholder="${field}"]`,user);
    await clickText(card+" button","Link");
    await wait(async ()=>(await accounts()).some(account=>account.provider===provider && account.linked),"profile did not link");
    const account=(await accounts()).find(account=>account.provider===provider);
    assert.equal(account.profile.username.toLowerCase(),user);
    await waitFor(()=>evaluate(`document.querySelector("${card}")?.textContent.toLowerCase().includes(${JSON.stringify(user)})`),"linked profile did not render");
    if (account.profile.rating) assert.ok(await evaluate(`document.querySelector("${card}")?.textContent.includes(${JSON.stringify(String(account.profile.rating))})`),"linked rating missing");
    await setField(".account-actions input[type=number]","1");
    await evaluate("window.onlineResult = null");
    await clickText(card+" button",provider==="lichess"?"Import games":"Import latest month");
    await wait(()=>evaluate('window.onlineResult !== null'),"linked import did not respond");
    const imported=await evaluate("window.onlineResult");
    assert.ok(imported.ok && imported.body.imported>0,JSON.stringify({provider,result:imported}));
    assert.equal(imported.body.imported,1,"linked import ignored the requested limit");
    await command("Page.reload");
    await view("home");
    await evaluate('document.querySelector(".home details").open = true');
    await waitFor(()=>evaluate(`document.querySelector("${card}")?.textContent.toLowerCase().includes(${JSON.stringify(user)})`),"linked account did not survive reload");
    await clickText(card+" button","Unlink");
    await wait(async ()=>(await accounts()).some(account=>account.provider===provider && !account.linked),"unlink did not persist");
    await command("Page.reload");
    await view("home");
    await evaluate('document.querySelector(".home details").open = true');
    await waitFor(()=>evaluate(`!!document.querySelector('${card} input[placeholder="${field}"]')`),"unlinked form did not restore");
    console.log(JSON.stringify({provider,username:user,linkedImportLimit:1}));
  }
  if (TOKEN) {
    await home();
    await observeOnline();
    const card=".account-card:nth-child(1)";
    // A rejected credential: clear failure, nothing linked, nothing stored.
    await setField(`${card} input[placeholder="Lichess username"]`,"thibault");
    await setField(`${card} input[type=password]`,"lip_definitely_not_a_valid_token");
    await clickText(card+" button","Link");
    await wait(()=>evaluate(`[...document.querySelectorAll("${card} button")].some(b=>b.textContent.trim()==="Link" && !b.disabled)`),"bad token request never finished");
    const failure=await evaluate('[...document.querySelectorAll(".home .muted")].map(e=>e.textContent).join(" | ")');
    assert.ok(/401|unauthori|invalid|rejected|http status/i.test(failure),"bad token gave no failure feedback: "+failure);
    assert.ok(!(await accounts()).some(account=>account.provider==="lichess" && account.linked),"bad token linked an account");

    // The approved credential: typed into the field, then profile, import, explorer.
    await home();
    await setField(`${card} input[placeholder="Lichess username"]`,"thibault");
    await setField(`${card} input[type=password]`,TOKEN);
    await clickText(card+" button","Link");
    await wait(async ()=>(await accounts()).some(account=>account.provider==="lichess" && account.linked && account.has_token),"token link did not persist");
    const listing=JSON.stringify(await accounts());
    assert.ok(!listing.includes(TOKEN),"accounts API echoed the token");
    assert.ok(!(await evaluate("document.documentElement.outerHTML")).includes(TOKEN),"page DOM contains the token");
    assert.ok(!(await (await fetch(new URL("/api/settings",base))).text()).includes(TOKEN),"settings API echoed the token");
    await setField(".account-actions input[type=number]","1");
    await evaluate("window.onlineResult = null");
    await clickText(card+" button","Import games");
    await wait(()=>evaluate('window.onlineResult !== null'),"authenticated import did not respond");
    const authed=await evaluate("window.onlineResult");
    assert.ok(authed.ok,"authenticated import failed: "+JSON.stringify(authed));
    const fen=encodeURIComponent("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
    for (const source of ["lichess","masters"]) {
      const explorer=await api(`openings?fen=${fen}&source=${source}`);
      assert.ok(explorer.continuations.length>0,source+" explorer returned no continuations with the token");
      const total=explorer.continuations.reduce((sum,move)=>sum+move.games,0);
      assert.ok(explorer.continuations.every(move=>move.games===move.white+move.draw+move.black),source+" counts do not add up");
      assert.ok(total>1000 && explorer.continuations.some(move=>move.san==="c5"),source+" live counts implausible for 1.e4: "+total);
    }
    await command("Page.reload");
    await view("home");
    await evaluate('document.querySelector(".home details").open = true');
    await clickText(card+" button","Unlink");
    await wait(async ()=>(await accounts()).some(account=>account.provider==="lichess" && !account.linked && !account.has_token),"unlink did not clear the token");
    console.log(JSON.stringify({authenticated:true,profile:"linked",explorerSources:["lichess","masters"],unlinkClearedToken:true}));
  }
  console.log("RBC_LIVE_ONLINE_ACCOUNTS_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_LIVE_ONLINE_ACCOUNTS_FAIL");
  process.exitCode=1;
});
