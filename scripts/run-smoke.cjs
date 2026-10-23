// Runs the existing native suites without a globally installed Playwright CLI.
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const wait=ms=>new Promise(r=>setTimeout(r,ms));
const executable=path.resolve(process.env.NORAM_EXE||'target/release/noram-browser.exe');
const root=fs.mkdtempSync(path.join(require('node:os').tmpdir(),'noram-qa-'));
let server,browser,app;const children=[];const results=[];
async function launch(profile,port,privateMode=false){
 const child=spawn(executable,['--profile',profile,'--background','--debug-port',String(port),...(privateMode?['--private']:[])],{windowsHide:true,stdio:['ignore','pipe','pipe']});children.push(child);app=child;
 let errors='';child.stderr.on('data',v=>{errors+=v;});
 for(let i=0;i<150;i++){
  if(child.exitCode!==null)throw Error('NoRAM exited: '+errors);
  try{browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);const shell=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url()==='http://noram-ui.localhost/');if(shell){await shell.waitForFunction(()=>typeof state!=='undefined'&&state!==null);return shell;}await browser.close();}catch{}
  await wait(100);
 }
 throw Error('NoRAM did not start: '+errors);
}
async function stop(shell){
 // Ask the host to flush its profile and release its WebView2 environments.
 if(shell)await shell.evaluate(()=>send('quit')).catch(()=>{});
 await wait(300);
 await browser?.close().catch(()=>{});
 if(app&&app.exitCode===null){await Promise.race([new Promise(r=>app.once('exit',r)),wait(3000)]);if(app.exitCode===null)execFileSync('taskkill',['/PID',String(app.pid),'/T','/F'],{windowsHide:true,stdio:'ignore'});}
 execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-File',path.resolve('scripts/cleanup-test-processes.ps1'),'-ProfileRoot',root],{windowsHide:true,stdio:'ignore'});
}
async function suite(name,shell){
 const source=fs.readFileSync('scripts/'+name+'.js','utf8');
 const fn=vm.runInThisContext('('+source+'\n)',{filename:name+'.js'});
 const result=await fn(shell);results.push({suite:name,result});console.log('PASS',name,JSON.stringify(result));
 fs.writeFileSync('output/playwright/results.json',JSON.stringify({status:'running',results},null,2));
}
(async()=>{
 fs.mkdirSync('output/playwright',{recursive:true});fs.mkdirSync('artifacts/readiness-downloads',{recursive:true});
 fs.writeFileSync('output/playwright/results.json',JSON.stringify({status:'running',results},null,2));
 server=spawn(process.execPath,['scripts/qa-server.cjs'],{windowsHide:true,stdio:'inherit'});children.push(server);await wait(500);
 execFileSync(process.execPath,['scripts/import-fixtures.cjs'],{stdio:'inherit'});
 execFileSync(process.execPath,['scripts/exporter-test.cjs'],{stdio:'inherit'});
 const profile=path.join(root,'normal');fs.mkdirSync(profile);
 fs.writeFileSync(path.join(profile,'browser.json'),JSON.stringify({settings:{auto_filter_updates:false,auto_app_updates:false}}));
 let shell=await launch(profile,9240);
 await suite('smoke',shell);
 // Preserve the original suite's restore fixture before subsequent tests add tabs.
 await shell.evaluate(()=>{const t=state.tabs.find(t=>t.url.includes('48621/two'));if(t&&!t.muted)send('mute',{target:t.id});});
 await wait(500);await stop(shell);
 shell=await launch(profile,9240);await suite('restore-smoke',shell);
 await suite('v02-smoke',shell);await suite('import-smoke',shell);await suite('readiness-smoke',shell);
 if(!fs.readFileSync('artifacts/readiness.pdf').subarray(0,5).equals(Buffer.from('%PDF-')))throw Error('Native PDF output is invalid');
 if(!fs.readdirSync('artifacts/readiness-downloads').some(n=>n.startsWith('noram-qa')&&n.endsWith('.bin')))throw Error('Native download output missing');
 const forwarded=spawn(executable,['--profile',profile,'--background','http://127.0.0.1:48621/forwarded'],{windowsHide:true,stdio:'ignore'});children.push(forwarded);
 await new Promise((resolve,reject)=>{forwarded.on('exit',code=>code===0?resolve():reject(Error('URL forwarding failed')));forwarded.on('error',reject);});
 await shell.waitForFunction(()=>state.tabs.some(t=>t.url.endsWith('/forwarded')));results.push({suite:'single-instance',result:'link forwarded to the existing browser'});

 await wait(500);await stop(shell);
 const before=fs.readFileSync(path.join(profile,'browser.json'));
 shell=await launch(profile,9250,true);
 await suite('import-private-smoke',shell);
 await shell.evaluate(()=>{send('new');send('navigate',{url:'http://127.0.0.1:48621/private'});});
 await shell.waitForFunction(()=>state.tabs.some(t=>t.url.includes('/private')&&!t.loading));
 await shell.evaluate(()=>send('bookmark'));await wait(250);await stop(shell);
 if(!before.equals(fs.readFileSync(path.join(profile,'browser.json'))))throw Error('Private mode modified normal profile');
 results.push({suite:'private-profile',result:'normal JSON unchanged'});
 fs.writeFileSync('output/playwright/results.json',JSON.stringify({status:'passed',results},null,2));
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{
 if(process.exitCode)fs.writeFileSync('output/playwright/results.json',JSON.stringify({status:'failed',results},null,2));
 for(const child of children)if(child.exitCode===null){try{execFileSync('taskkill',['/PID',String(child.pid),'/T','/F'],{windowsHide:true,stdio:'ignore'});}catch{}}
 try{execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-File',path.resolve('scripts/cleanup-test-processes.ps1'),'-ProfileRoot',root],{windowsHide:true,stdio:'ignore'});}catch{}
 // Keep disposable profiles for debugging; only paths created by this runner are used.
 console.log('QA profiles:',root);
});
