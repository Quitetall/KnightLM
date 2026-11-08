const { openView } = require("./navigation");
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9532);
function get(p){return new Promise((res,rej)=>{http.get({host:"127.0.0.1",port:PORT,path:p},r=>{let d="";r.on("data",c=>d+=c);r.on("end",()=>res(JSON.parse(d)));}).on("error",rej);});}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
  const chrome=spawn("google-chrome",["--headless=new","--disable-gpu","--no-sandbox",`--remote-debugging-port=${PORT}`,`--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-rail2-${Date.now()}`,"--remote-allow-origins=*","--window-size=1500,1000",(process.env.RBC_CDP_URL || "http://127.0.0.1:8090/")],{stdio:["ignore","ignore","ignore"]});
  let t;for(let i=0;i<40;i++){await sleep(300);try{t=await get("/json/list");break;}catch{}}
  const ws=new WebSocket(t.find(x=>x.type==="page").webSocketDebuggerUrl);
  let id=0;const pend=new Map();const send=(m,pr={})=>new Promise(r=>{const i=++id;pend.set(i,r);ws.send(JSON.stringify({id:i,method:m,params:pr}));});
  ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pend.has(m.id)){pend.get(m.id)(m.result);pend.delete(m.id);}};
  await new Promise(r=>ws.onopen=r); await send("Runtime.enable"); await sleep(2800);
  const ev=async(x)=>{const r=await send("Runtime.evaluate",{expression:x,returnByValue:true,awaitPromise:true});return r.result?.value;};
  const clickText=async(sel,txt)=>await ev(`(()=>{const e=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(txt)});e&&e.click();return !!e;})()`);

  await openView(ev, "workbench", "assistant"); await sleep(900);
  await clickText("button","New chat"); await sleep(1500);

  // Rename: the ✎ button opens an input; save writes it back to the rail.
  const opened=await ev(`(()=>{const b=document.querySelector('button[aria-label="Rename"]');if(!b)return false;b.click();return true;})()`);
  await sleep(400);
  const typed=await ev(`(()=>{const i=document.querySelector('input[aria-label="Rename"]');if(!i)return false;i.value="Rail rename test";i.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  await sleep(200);
  const saved=await clickText("button","Save"); await sleep(1200);
  const renamed=await ev(`[...document.querySelectorAll(".dock-tab")].some(x=>x.textContent.includes("Rail rename test"))`);

  const beforeArchive=await ev(`document.querySelectorAll(".dock-tab").length`);
  // Archive: the × button removes it from the rail.
  const archived=await ev(`(()=>{const b=document.querySelector('button[aria-label="Archive"]');if(!b)return false;b.click();return true;})()`);
  await sleep(1300);
  const remaining=await ev(`document.querySelectorAll(".dock-tab").length`);

  const out={opened,typed,saved,renamed,archived,beforeArchive,remaining};
  console.log(JSON.stringify(out));
  const ok = opened && typed && saved && renamed && archived && remaining===beforeArchive-1;
  console.log(ok?"AI_RAIL2_PASS":"AI_RAIL2_FAIL");
  ws.close();chrome.kill();process.exit(ok?0:1);
})().catch(e=>{console.error(e);process.exit(1);});
