// Run with: playwright-cli -s=noram-smoke run-code --filename scripts/smoke.js
// Attach ONLY to an isolated --background instance; this changes its test profile.
async (page) => {
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const read = () => page.evaluate(() => ({ live: state.live, tabs: state.tabs, blocked: state.blocked, active: state.active, settings: state.settings, bookmarks: state.bookmarks, history: state.history, message: state.message }));
  const command = (cmd, extra = {}) => page.evaluate(({cmd,extra}) => send(cmd,extra), {cmd,extra});
  const until = async (predicate, description) => {
    const deadline = Date.now()+15000;
    while (Date.now()<deadline) { const value=await read(); if(predicate(value))return value; await page.waitForTimeout(100); }
    throw new Error('Timed out: '+description);
  };
  assert(page.url()==='http://noram-ui.localhost/', 'Select the trusted NoRAM shell before running the test');
  const errors=[];page.on('pageerror', e=>errors.push(e.message));
  await page.getByRole('textbox',{name:'Address or search',exact:true}).fill('http://127.0.0.1:48621/one?utm_source=smoke&q=kept');
  await page.getByRole('textbox',{name:'Address or search',exact:true}).press('Enter');
  await until(s=>s.tabs.some(t=>t.url==='http://127.0.0.1:48621/one?q=kept'&&!t.loading),'native page navigation');
  const contentBrowser=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9241');
  let content=contentBrowser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('48621/one'));
  assert(content,'Content webview exists');
  assert(await content.getByRole('heading',{name:'Native browser fixture'}).isVisible(),'Real content rendered');
  const headers=await content.evaluate(async()=>await(await fetch('/headers')).json());
  assert(headers.dnt==='1'&&headers['sec-gpc']==='1','Privacy request headers arrived at real HTTP server');
  const permission=await content.evaluate(()=>new Promise(resolve=>navigator.geolocation.getCurrentPosition(()=>resolve('allowed'),error=>resolve(error.code),{timeout:5000})));
  assert(permission===1,'Geolocation permission denied by native policy');
  await content.evaluate(()=>localStorage.setItem('noram-qa-normal','normal-only'));
  const blockBefore=(await read()).blocked;
  const blockedResponse=content.waitForResponse(r=>r.url().includes('ads.noram.test/noram-smoke'),{timeout:10000});
  await content.evaluate(()=>{const img=new Image();img.src='https://ads.noram.test/noram-smoke';document.body.append(img);});
  assert((await blockedResponse).status()===403,'Native request blocker returned 403');
  await until(s=>s.blocked>blockBefore,'native blocked counter');
  await command('bookmark');
  await until(s=>s.bookmarks.some(b=>b.url.includes('/one?q=kept')),'bookmark persistence');
  await content.getByRole('link',{name:'Second page',exact:true}).click();
  await content.waitForURL('**/two');
  await command('back'); await content.waitForURL('**/one?q=kept');
  await command('forward'); await content.waitForURL('**/two');
  const beforePopup=(await read()).tabs.length;
  // Playwright's evaluate sets CDP userGesture=true. Let transient activation expire
  // before the timer fires so this exercises an actual unsolicited popup.
  await content.evaluate(()=>{setTimeout(()=>window.open('/scripted'),6000);});
  await page.waitForTimeout(6500);
  assert((await read()).tabs.length===beforePopup,'Scripted popup denied');
  await content.getByRole('link',{name:'Open new tab',exact:true}).click();
  await until(s=>s.tabs.length===beforePopup+1,'user-requested link opened a tab');
  const currentSettings=(await read()).settings;
  await command('settings',{settings:{...currentSettings,max_live_tabs:1}});
  await until(s=>s.live===1&&s.tabs.some(t=>t.sleeping),'least recently used tab unloaded');
  const sleeping=(await read()).tabs.find(t=>t.sleeping);
  await command('switch',{id:sleeping.id});
  await until(s=>s.active===sleeping.id&&s.tabs.find(t=>t.id===sleeping.id)?.sleeping===false&&s.live===1,'sleeping tab resumed within budget');
  await command('pin');
  await command('new');await command('navigate',{url:'http://127.0.0.1:48621/pinned-check'});
  await until(s=>s.live===2,'pinned tab exemption');
  await command('navigate',{url:'javascript:alert(1)'});
  await until(s=>s.message.includes('Only HTTP'),'unsafe navigation rejected');
  await command('navigate',{url:'http://example.com/?utm_source=smoke'});
  await until(s=>s.tabs.find(t=>t.id===s.active)?.url==='https://example.com/','HTTPS upgrade and parameter cleaning');
  await command('new');
  assert(errors.length===0,'No trusted UI JavaScript errors: '+errors.join(', '));
  await page.screenshot({path:'output/playwright/start-page.png'});
  await contentBrowser.close();
  return {passed:['native rendering','URL cleanup','DNT and GPC server headers','geolocation denial','network blocker HTTP 403','bookmark','back/forward','scripted-popup denial','user popup tab','tab unloading/resume','pinned exemption','unsafe scheme rejection','HTTPS upgrade','UI without JavaScript errors'],state:await read()};
}
