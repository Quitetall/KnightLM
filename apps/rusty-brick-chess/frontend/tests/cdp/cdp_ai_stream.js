// Streaming and stop: text must appear while the turn runs, and Stop must end it.
const { spawn } = require("child_process");
const http = require("http");
const PORT = 9534;
function get(p){return new Promise((res,rej)=>{http.get({host:"127.0.0.1",port:PORT,path:p},r=>{let d="";r.on("data",c=>d+=c);r.on("end",()=>res(JSON.parse(d)));}).on("error",rej);});}
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
  const chrome=spawn("google-chrome",["--headless=new","--disable-gpu","--no-sandbox",`--remote-debugging-port=${PORT}`,`--user-data-dir=/tmp/chrome-stream-${Date.now()}`,"--remote-allow-origins=*","--window-size=1500,1000","http://127.0.0.1:8090/"],{stdio:["ignore","ignore","ignore"]});
  let t;for(let i=0;i<40;i++){await sleep(300);try{t=await get("/json/list");break;}catch{}}
  const ws=new WebSocket(t.find(x=>x.type==="page").webSocketDebuggerUrl);
  let id=0;const pend=new Map();const send=(m,pr={})=>new Promise(r=>{const i=++id;pend.set(i,r);ws.send(JSON.stringify({id:i,method:m,params:pr}));});
  ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.id&&pend.has(m.id)){pend.get(m.id)(m.result);pend.delete(m.id);}};
  await new Promise(r=>ws.onopen=r); await send("Runtime.enable"); await sleep(2800);
  const ev=async(x)=>{const r=await send("Runtime.evaluate",{expression:x,returnByValue:true,awaitPromise:true});return r.result?.value;};
  const clickText=async(sel,txt)=>await ev(`(()=>{const e=[...document.querySelectorAll(${JSON.stringify(sel)})].find(x=>x.textContent.trim()===${JSON.stringify(txt)});e&&e.click();return !!e;})()`);

  await clickText(".nav","Assistant"); await sleep(900);
  await clickText("button","New chat"); await sleep(1500);
  const typed=await ev(`(()=>{const t=document.querySelector(".chat-input textarea");if(!t)return false;t.value="stream please";t.dispatchEvent(new Event("input",{bubbles:true}));return true;})()`);
  await sleep(200);
  const started=await clickText(".chat-input button","Send");
  // Wait for the first delta to land rather than sampling at a fixed instant:
  // a slow provider may not have sent anything yet.
  let partial="";
  for(let i=0;i<12 && !partial;i++){await sleep(250);partial=await ev(`(()=>{const b=document.querySelector(".chat-msg.streaming");return b?b.textContent.trim():"";})()`);}
  const buttonLabel=await ev(`document.querySelector(".chat-input button")?.textContent.trim()`);
  const stopped=await clickText(".chat-input button","Stop");
  await sleep(2500);
  const after=await ev(`(()=>{const b=document.querySelector(".chat-msg.streaming");return b?b.textContent.trim():"";})()`);
  const errorText=await ev(`(()=>{const e=document.querySelector(".chat-msg.error");return e?e.textContent.trim():"";})()`);
  const labelAfter=await ev(`document.querySelector(".chat-input button")?.textContent.trim()`);

  console.log(JSON.stringify({typed,started,partial,buttonLabel,stopped,after,errorText,labelAfter}));
  const ok = typed && started && partial.length>0 && buttonLabel==="Stop" && stopped && labelAfter==="Send";
  console.log(ok?"AI_STREAM_PASS":"AI_STREAM_FAIL");
  ws.close();chrome.kill();process.exit(ok?0:1);
})().catch(e=>{console.error(e);process.exit(1);});
