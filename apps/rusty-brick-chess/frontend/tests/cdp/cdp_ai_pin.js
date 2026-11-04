// Pin an assistant conversation into a tab of its own.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9533;
function get(p){return new Promise((res,rej)=>{http.get({host:"127.0.0.1",port:PORT,path:p},r=>{let d="";r.on("data",c=>d+=c);r.on("end",()=>res(JSON.parse(d)));}).on("error",rej);});}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
  const chrome=spawn("google-chrome",["--headless=new","--disable-gpu","--no-sandbox",`--remote-debugging-port=${PORT}`,`--user-data-dir=/tmp/chrome-pin-${Date.now()}`,"--remote-allow-origins=*","--window-size=1500,1000","http://127.0.0.1:8090/"],{stdio:["ignore","ignore","ignore"]});
  let t;for(let i=0;i<40;i++){await sleep(300);try{t=await get("/json/list");break;}catch{}}
  const ws=new WebSocket(t.find(x=>x.type==="page").webSocketDebuggerUrl);
  let id=0;const pend=new Map();const send=(m,pr={})=>new Promise(r=>{const i=++id;pend.set(i,r);ws.send(JSON.stringify({id:i,method:m,params:pr}));});
  ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pend.has(m.id)){pend.get(m.id)(m.result);pend.delete(m.id);}};
  await new Promise(r=>ws.onopen=r); await send("Runtime.enable"); await sleep(2800);
  const ev=async(x)=>{const r=await send("Runtime.evaluate",{expression:x,returnByValue:true,awaitPromise:true});return r.result?.value;};
  const clickText=async(sel,txt)=>await ev(`(()=>{const e=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(txt)});e&&e.click();return !!e;})()`);

  await clickText(".nav","Assistant"); await sleep(900);
  await clickText("button","New chat"); await sleep(1400);
  const tabsBefore=await ev(`document.querySelectorAll(".ws-tab").length`);
  const pinned=await ev(`(()=>{const b=document.querySelector('button[aria-label="Open in its own tab"]');if(!b)return false;b.click();return true;})()`);
  await sleep(1200);
  const tabsAfter=await ev(`document.querySelectorAll(".ws-tab").length`);
  const stillAssistant=await ev(`!!document.querySelector('input[type="search"]')`);
  const railRows=await ev(`document.querySelectorAll(".dock-tab").length`);

  // Pinning the same conversation again focuses the tab it already has.
  const pinnedAgain=await ev(`(()=>{const b=document.querySelector('button[aria-label="Open in its own tab"]');if(!b)return false;b.click();return true;})()`);
  await sleep(900);
  const tabsTwice=await ev(`document.querySelectorAll(".ws-tab").length`);

  console.log(JSON.stringify({tabsBefore,pinned,tabsAfter,stillAssistant,railRows,pinnedAgain,tabsTwice}));
  const ok = pinned && stillAssistant && tabsAfter===tabsBefore+1 && pinnedAgain && tabsTwice===tabsAfter && railRows>0;
  console.log(ok?"AI_PIN_PASS":"AI_PIN_FAIL");
  ws.close();chrome.kill();process.exit(ok?0:1);
})().catch(e=>{console.error(e);process.exit(1);});
