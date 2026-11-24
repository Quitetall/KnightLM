const assert = require("node:assert/strict");
const { withBrowser, waitFor } = require("./browser");

withBrowser(async ({ base, command, evaluate, click, setField, destination, view }) => {
  await view("home");
  await destination("settings");
  await view("settings");
  const providerUrl = "http://127.0.0.1:" + process.env.RBC_CDP_PROVIDER_PORT + "/v1";
  const providers = async () => {
    const response=await fetch(new URL("/api/providers",base));
    const text=await response.text();
    assert.ok(!text.includes(process.env.RBC_PARITY_PROVIDER_KEY),"registry exposed credential material");
    return JSON.parse(text).providers;
  };
  const add = async (label,url,ref) => {
    await setField(".provider-label",label);
    await setField(".provider-url",url);
    await setField(".provider-key-ref",ref);
    await click(".provider-add-btn");
    await waitFor(async ()=>(await providers()).some(row=>row.label===label),"provider was not added");
    const id=(await providers()).find(row=>row.label===label).id;
    await waitFor(()=>evaluate(`!!document.querySelector('[data-provider="${id}"]')`),"provider row did not render");
    return id;
  };
  const row = id => `[data-provider="${id}"]`;
  const test = async (id,ok) => {
    await evaluate("window.providerProbe = null");
    await click(row(id)+" .provider-test");
    await waitFor(()=>evaluate('!!window.providerProbe'),"provider connection test did not complete");
    assert.equal(await evaluate("window.providerProbe.ok"),ok);
    await waitFor(()=>evaluate(`document.querySelector(".settings-providers")?.textContent.includes("${ok?"ok:":"failed:"}")`),"test result missing");
  };
  await evaluate(`(() => {
    const original=window.fetch.bind(window);
    window.fetch=async (...args)=>{
      const response=await original(...args);
      const path=new URL(args[0].url || args[0],location.href).pathname;
      if (path.startsWith("/api/providers/") && path.endsWith("/test")) window.providerProbe=await response.clone().json();
      return response;
    };
  })()`);
  const missing=await add("Missing reference",providerUrl,"RBC_PARITY_PROVIDER_KEY_MISSING");
  assert.equal((await providers()).find(row=>row.id===missing).key_resolves,false);
  await test(missing,false);
  await click(row(missing)+" .provider-delete");
  await waitFor(async ()=>(await providers()).every(row=>row.id!==missing),"missing provider was not removed");

  const valid=await add("Registry fixture",providerUrl,"RBC_PARITY_PROVIDER_KEY");
  assert.equal((await providers()).find(row=>row.id===valid).key_resolves,true);
  await test(valid,true);
  await click(row(valid)+" .provider-refresh");
  await waitFor(async ()=>(await providers()).find(row=>row.id===valid)?.models.length===2,"model discovery did not persist");
  await waitFor(()=>evaluate(`document.querySelector('${row(valid)} .provider-model-list')?.textContent.includes("vision") && document.querySelector('${row(valid)} .provider-model-list')?.textContent.includes("32768")`),"model capabilities missing");
  assert.equal((await providers()).find(row=>row.id===valid).models.find(model=>model.id==="o3-mini").reasoning,true);
  await setField('input[placeholder="http://127.0.0.1:8000/v1/chat"]',providerUrl+"/chat/completions");
  await evaluate('[...document.querySelectorAll(".settings-form button")].find(button=>button.textContent.trim()==="Save").click()');
  await waitFor(()=>evaluate(`fetch("/api/settings").then(r=>r.json()).then(data=>data.model_url===${JSON.stringify(providerUrl+"/chat/completions")})`),"default model URL did not persist");
  const unreachable=await add("Unavailable fixture",new URL("/unavailable-provider",base).href,"");
  await test(unreachable,false);
  await click(row(unreachable)+" .provider-delete");

  await click(".provider-scan");
  await waitFor(()=>evaluate('!!document.querySelector(".local-endpoint.found")'),"no responsive local model endpoint was discovered");
  const endpoint=await evaluate('document.querySelector(".local-endpoint.found")?.textContent');
  await click(".local-endpoint.found .local-add");
  await waitFor(async ()=>(await providers()).some(row=>endpoint.includes(row.label) && row.id!==valid),"responsive endpoint was not added");
  const local=(await providers()).find(row=>endpoint.includes(row.label) && row.id!==valid);
  assert.equal(local.key_ref,null);
  await command("Page.reload");
  await view("settings");
  await waitFor(()=>evaluate(`!!document.querySelector('${row(valid)}') && !!document.querySelector('${row(local.id)}')`),"registry did not restore");
  await click(row(valid)+" .provider-delete");
  await click(row(local.id)+" .provider-delete");
  await waitFor(async ()=>(await providers()).every(row=>row.id!==valid && row.id!==local.id),"provider deletion did not persist");
  await command("Page.reload");
  await view("settings");
  await waitFor(()=>evaluate(`!document.querySelector('${row(valid)}') && !document.querySelector('${row(local.id)}')`),"removed providers returned after reload");
  console.log("RBC_PROVIDER_SETTINGS_PASS");
}).catch(error=>{
  console.error(error);
  console.log("RBC_PROVIDER_SETTINGS_FAIL");
  process.exitCode=1;
});
