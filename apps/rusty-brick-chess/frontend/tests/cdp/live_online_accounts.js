const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

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
  console.log("RBC_LIVE_ONLINE_ACCOUNTS_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_LIVE_ONLINE_ACCOUNTS_FAIL");
  process.exitCode=1;
});
