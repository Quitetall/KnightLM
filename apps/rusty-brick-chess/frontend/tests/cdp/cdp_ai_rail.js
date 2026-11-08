const { openView } = require("./navigation");
const { spawn } = require("child_process");
const http = require("http");
const PORT = Number(process.env.RBC_CDP_PORT || 9531);
const BASE = (process.env.RBC_CDP_URL || "http://127.0.0.1:8090/");
function get(p){return new Promise((res,rej)=>{http.get({host:"127.0.0.1",port:PORT,path:p},r=>{let d="";r.on("data",c=>d+=c);r.on("end",()=>res(JSON.parse(d)));}).on("error",rej);});}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
  const chrome=spawn("google-chrome",["--headless=new","--disable-gpu","--no-sandbox",`--remote-debugging-port=${PORT}`,`--user-data-dir=${process.env.TMPDIR || "/tmp"}/chrome-rail-${Date.now()}`,"--remote-allow-origins=*","--window-size=1500,1000",BASE],{stdio:["ignore","ignore","ignore"]});
  let t;for(let i=0;i<40;i++){await sleep(300);try{t=await get("/json/list");break;}catch{}}
  const ws=new WebSocket(t.find(x=>x.type==="page").webSocketDebuggerUrl);
  let id=0;const pend=new Map();const send=(m,pr={})=>new Promise(r=>{const i=++id;pend.set(i,r);ws.send(JSON.stringify({id:i,method:m,params:pr}));});
  ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pend.has(m.id)){pend.get(m.id)(m.result);pend.delete(m.id);}};
  await new Promise(r=>ws.onopen=r); await send("Runtime.enable"); await send("Page.enable"); await sleep(2800);
  const ev=async(x)=>{const r=await send("Runtime.evaluate",{expression:x,returnByValue:true,awaitPromise:true});return r.result?.value;};
  const clickText=async(sel,txt)=>await ev(`(()=>{const e=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(txt)});e&&e.click();return !!e;})()`);

  const nav=await openView(ev, "workbench", "assistant"); await sleep(900);
  const newChat=await clickText("button","New chat"); await sleep(1500);

  // The rail has a search box and exactly one row per conversation.
  const railCount=async()=>await ev(`document.querySelectorAll(".dock-tab").length`);
  const before=await railCount();
  const hasSearch=await ev(`(()=>{const i=document.querySelector('input[type="search"]');if(!i)return false;i.value="zzz-no-such-chat";i.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  await sleep(500);
  const filtered=await railCount();
  const restoredText=await ev(`(()=>{const i=document.querySelector('input[type="search"]');i.value="";i.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  await sleep(400);
  const afterClear=await railCount();

  // The session that was open survives a reload.
  const selectedBefore=await ev(`(()=>{const e=document.querySelector(".dock-tab.active");return e?e.textContent.trim():"";})()`);
  await send("Page.reload",{}); await sleep(3200);
  await openView(ev, "workbench", "assistant"); await sleep(1200);
  const selectedAfter=await ev(`(()=>{const e=document.querySelector(".dock-tab.active");return e?e.textContent.trim():"";})()`);

  const out={nav,newChat,before,hasSearch,filtered,afterClear,selectedBefore,selectedAfter};
  console.log(JSON.stringify(out));
  const ok = nav && newChat && hasSearch && before>0 && filtered===0 && afterClear===before
    && selectedBefore.length>0 && selectedBefore===selectedAfter;
  console.log(ok?"AI_RAIL_PASS":"AI_RAIL_FAIL");
  ws.close();chrome.kill();process.exit(ok?0:1);
})().catch(e=>{console.error(e);process.exit(1);});
