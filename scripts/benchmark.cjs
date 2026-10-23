// Isolated synthetic workload. Does not attach to personal browsers or profiles.
if(!process.argv.includes('--run')){console.log('Add --run to start isolated benchmark browsers. No browsers were launched.');process.exit(0);}
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),http=require('node:http');
const wait=ms=>new Promise(r=>setTimeout(r,ms));
const argument=(name,fallback)=>{const i=process.argv.indexOf('--'+name);return i<0?fallback:process.argv[i+1];};
const tabs=Number(argument('tabs','10')),runs=Number(argument('runs','3')),idle=Number(argument('idle-ms','3000'));
if(!Number.isInteger(tabs)||tabs<2||tabs>100||!Number.isInteger(runs)||runs<1||runs>20||idle<0||idle>60000)throw Error('Invalid benchmark arguments');
const executable=path.resolve(process.env.NORAM_EXE||'target/release/noram-browser.exe');
const root=fs.mkdtempSync(path.join(os.tmpdir(),'noram-bench-'));
const candidates=[['NoRAM',executable],['Edge',path.join(process.env['ProgramFiles(x86)']||'C:/Program Files (x86)','Microsoft/Edge/Application/msedge.exe')],['Chrome',path.join(process.env.ProgramFiles||'C:/Program Files','Google/Chrome/Application/chrome.exe')],['Brave',path.join(process.env.ProgramFiles||'C:/Program Files','BraveSoftware/Brave-Browser/Application/brave.exe')]];
const browsers=candidates.filter(([,p])=>fs.existsSync(p));
const results=[];let child,connection,websites;
const fixture=http.createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.setHeader('Cache-Control','no-store');res.end('<!doctype html><title>Synthetic benchmark</title><h1>Identical local workload</h1><script>window.work=new Uint8Array(4*1024*1024);for(let i=0;i<work.length;i+=4096)work[i]=1;</script>');});
const sample=pid=>JSON.parse(execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-File',path.resolve('scripts/measure-process-tree.ps1'),'-RootProcessId',String(pid)],{windowsHide:true,encoding:'utf8'}));
const median=values=>{const v=[...values].sort((a,b)=>a-b);return v[Math.floor(v.length/2)];};
async function stop(){
 await websites?.close().catch(()=>{});await connection?.close().catch(()=>{});websites=null;connection=null;
 if(child&&child.exitCode===null){try{execFileSync('taskkill',['/PID',String(child.pid),'/T','/F'],{windowsHide:true,stdio:'ignore'});}catch{}}
 child=null;
 execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-File',path.resolve('scripts/cleanup-test-processes.ps1'),'-ProfileRoot',root],{windowsHide:true,stdio:'ignore'});
}
(async()=>{
 await new Promise(r=>fixture.listen(0,'127.0.0.1',r));const port=fixture.address().port;
 for(let run=0;run<runs;run++)for(const[name,exe]of browsers){
  for(const budget of name==='NoRAM'?[4,tabs]:[null]){
   const profile=path.join(root,name+'-'+run+'-'+budget);fs.mkdirSync(profile);
   const debugPort=9350;
   if(name==='NoRAM')fs.writeFileSync(path.join(profile,'browser.json'),JSON.stringify({settings:{auto_app_updates:false,auto_filter_updates:false,max_live_tabs:budget}}));
   const args=name==='NoRAM'?['--background','--profile',profile,'--debug-port',String(debugPort)]:['--headless=new','--no-first-run','--no-default-browser-check','--user-data-dir='+profile,'--remote-debugging-port='+debugPort,'--window-size=1280,850','about:blank'];
   const started=performance.now();child=spawn(exe,args,{windowsHide:true,stdio:'ignore'});
   for(let n=0;n<200;n++){try{connection=await chromium.connectOverCDP('http://127.0.0.1:'+debugPort);break;}catch{if(child.exitCode!==null)throw Error(name+' exited');await wait(100);}}
   if(!connection)throw Error(name+' startup timed out');
   let shell=null;
   if(name==='NoRAM'){
    for(let n=0;n<150;n++){shell=connection.contexts().flatMap(c=>c.pages()).find(p=>p.url()==='http://noram-ui.localhost/');if(shell)break;await wait(100);}
    if(!shell)throw Error('NoRAM trusted shell did not load; refusing an invalid benchmark');
    await shell.waitForFunction(()=>typeof state!=='undefined'&&state!==null);
   }
   const readyMs=performance.now()-started,context=connection.contexts()[0];const pages=[];const pageLoadMs=[];
   for(let i=0;i<tabs;i++){
    const url='http://127.0.0.1:'+port+'/tab-'+i;const start=performance.now();
    if(shell){await shell.evaluate(url=>{send('new');send('navigate',{url});},url);await shell.waitForFunction(url=>state.tabs.some(t=>t.url===url&&!t.loading),url);}
    else {const page=await context.newPage();await page.goto(url);pages.push(page);}
    pageLoadMs.push(performance.now()-start);
   }
   await wait(idle);
   const before=sample(child.pid);await wait(idle);const after=sample(child.pid);
   const switchMs=[];
   if(shell){const ids=await shell.evaluate(()=>state.tabs.filter(t=>t.url.startsWith('http')).map(t=>t.id));for(const id of ids){const start=performance.now();await shell.evaluate(id=>send('switch',{id}),id);await shell.waitForFunction(id=>state.active===id&&state.tabs.some(t=>t.id===id&&!t.loading&&!t.sleeping),id);switchMs.push(performance.now()-start);}}
   else for(const page of pages){const start=performance.now();await page.bringToFront();await page.evaluate(()=>new Promise(requestAnimationFrame));switchMs.push(performance.now()-start);}
   const record={browser:name,version:await connection.version(),run:run+1,budget,tabCount:tabs,mode:shell?'hidden WebView2':'headless Chromium',readyMs,pageLoadMedianMs:median(pageLoadMs),tabActivationMedianMs:median(switchMs),tabActivationP95Ms:[...switchMs].sort((a,b)=>a-b)[Math.min(switchMs.length-1,Math.floor(switchMs.length*.95))],idleCpuSeconds:after.cpuSeconds-before.cpuSeconds,...after};
   results.push(record);console.log(JSON.stringify(record));await stop();
  }
 }
 fs.mkdirSync('output/benchmarks',{recursive:true});fs.writeFileSync('output/benchmarks/latest.json',JSON.stringify({timestamp:new Date().toISOString(),os:os.release(),cpu:os.cpus()[0].model,totalSystemMemory:os.totalmem(),runs,tabs,idleMs:idle,skipped:candidates.filter(([,p])=>!fs.existsSync(p)).map(([n])=>n),limitations:['Synthetic local pages with 4 MiB touched allocations; not representative of the wider web.','NoRAM hidden WebView2 versus headless Chromium; rendering modes differ.','Working-set sums include shared pages more than once; private bytes are committed memory, not private resident RAM.','Fresh browser profiles; operating-system file cache is uncontrolled.','Idle CPU is a process-tree cumulative counter difference; process exits can distort it.','CDP and automation overhead affect readiness and activation timing.'],results},null,2));
})().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{await stop();fixture.close();console.log('Benchmark profiles:',root);});
