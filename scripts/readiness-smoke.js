async(page)=>{
 const assert=(c,m)=>{if(!c)throw Error(m);};
 const cmd=(cmd,extra={})=>page.evaluate(({cmd,extra})=>send(cmd,extra),{cmd,extra});
 const until=async fn=>{await page.waitForFunction(fn,null,{timeout:15000});return page.evaluate(()=>state);};
 // Keep this suite independent of earlier tab fixtures.
 await cmd('new');
 const home=await page.evaluate(()=>state.active);
 await page.evaluate(home=>{for(const t of state.tabs)if(t.id!==home)send('close',{id:t.id});},home);
 await cmd('settings',{settings:{...(await page.evaluate(()=>state.settings)),max_live_tabs:1}});
 await cmd('navigate',{url:'http://127.0.0.1:48621/form'});
 await until(()=>state.tabs.some(t=>t.url.endsWith('/form')&&!t.loading));
 const formId=await page.evaluate(()=>state.active);
 const websites=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9241');
 let content=websites.contexts().flatMap(c=>c.pages()).find(p=>p.url().endsWith('/form'));
 await content.getByPlaceholder('Unsaved work').fill('Do not discard this');
 const tabCount=await page.evaluate(()=>state.tabs.length);
 await content.evaluate(()=>window.ipc?.postMessage(JSON.stringify({cmd:'new'})));await page.waitForTimeout(150);
 assert(await page.evaluate(()=>state.tabs.length)===tabCount,'Website cannot invoke native commands');
 await cmd('new');await cmd('navigate',{url:'http://127.0.0.1:48621/safe'});
 await until(()=>state.tabs.some(t=>t.url.endsWith('/safe')&&!t.loading));
 await page.waitForTimeout(500);
 assert(!(await page.evaluate(id=>state.tabs.find(t=>t.id===id).sleeping,formId)),'Form tab protected over budget');
 assert(await content.getByPlaceholder('Unsaved work').inputValue()==='Do not discard this','Form content retained');
 await cmd('switch',{id:formId});
 const pending=content.evaluate(()=>new Promise(resolve=>navigator.geolocation.getCurrentPosition(()=>resolve(0),e=>resolve(e.code),{timeout:10000})));
 await until(()=>state.permissionRequests.length>0);
 assert(await page.getByRole('dialog').isVisible(),'Permission prompt is visible');
 await page.getByRole('button',{name:'Always block',exact:true}).click();
 assert(await pending===1,'Native permission denied after prompt');
 await until(()=>state.settings.permissions['http://127.0.0.1:48621']?.location===false);
 const repeat=await content.evaluate(()=>new Promise(resolve=>navigator.geolocation.getCurrentPosition(()=>resolve(0),e=>resolve(e.code),{timeout:5000})));
 assert(repeat===1,'Remembered denial applies');
 assert(await page.evaluate(()=>state.permissionRequests.length)===0,'Remembered decision does not prompt again');
 await cmd('permission_reset',{origin:'http://127.0.0.1:48621'});
 await until(()=>!state.settings.permissions['http://127.0.0.1:48621']);
 // Test protection for media elements even before playback starts.
 await cmd('navigate',{url:'http://127.0.0.1:48621/media'});await content.waitForURL('**/media');await content.waitForLoadState('load');
 await content.evaluate(()=>document.body.append(document.createElement('video')));
 await cmd('new');await cmd('navigate',{url:'http://127.0.0.1:48621/other'});
 await until(()=>state.tabs.some(t=>t.url.endsWith('/other')&&!t.loading));await page.waitForTimeout(300);
 assert(!(await page.evaluate(id=>state.tabs.find(t=>t.id===id).sleeping,formId)),'Media tab protected');
 await cmd('switch',{id:formId});
 await content.evaluate(()=>{document.querySelector('video').remove();document.body.style.height='4000px';scrollTo(0,700);});
 await cmd('new');await cmd('navigate',{url:'http://127.0.0.1:48621/scroll-trigger'});
 await until(()=>state.tabs.some(t=>t.url.endsWith('/media')&&t.sleeping));
 await cmd('switch',{id:formId});await until(()=>state.tabs.some(t=>t.id===state.active&&!t.loading&&!t.sleeping));
 // A normal fixture needs height to restore scroll; the fixture serves a tall media page.
 content=websites.contexts().flatMap(c=>c.pages()).find(p=>p.url().endsWith('/media'));
 await content.waitForFunction(()=>scrollY>=690);

 await cmd('save_pdf',{path:'artifacts/readiness.pdf'});
 await until(()=>state.message==='PDF saved.');
 await cmd('download_directory',{path:'artifacts/readiness-downloads'});
 await content.evaluate(()=>{const a=document.createElement('a');a.href='/slow-download';a.textContent='Test download';document.body.append(a);});
 await content.getByRole('link',{name:'Test download',exact:true}).click();
 await until(()=>state.tabs.some(t=>t.id===state.active&&t.protected));
 await cmd('new');await cmd('navigate',{url:'http://127.0.0.1:48621/download-trigger'});
 await until(()=>state.tabs.some(t=>t.url.endsWith('/download-trigger')&&!t.loading));
 assert(!(await page.evaluate(id=>state.tabs.find(t=>t.id===id).sleeping,formId)),'Active download protected from unloading');
 await page.waitForTimeout(4000);
 // A crashed renderer should leave a recoverable tab, never silently loop reloads.
 await cmd('switch',{id:formId});await until(()=>state.tabs.some(t=>t.id===state.active&&!t.loading&&!t.sleeping));
 content=websites.contexts().flatMap(c=>c.pages()).find(p=>p.url().endsWith('/media'));
 const session=await content.context().newCDPSession(content);session.send('Page.crash').catch(()=>{});
 await until(()=>state.tabs.some(t=>t.id===state.active&&t.crashed));
 await page.getByRole('button',{name:'Reload tab',exact:true}).click();
 await until(()=>state.tabs.some(t=>t.id===state.active&&!t.crashed&&!t.sleeping&&!t.loading));
 // Keyboard-only tab selection uses one roving tab stop and arrow keys.
 const selected=page.getByRole('tab',{selected:true});await selected.focus();await selected.press('Home');
 assert(await page.getByRole('tab',{selected:true}).count()===1,'One selected accessible tab');
 assert(await page.locator('#tabs .tab-label[tabindex="0"]').count()===1,'One tab in keyboard tab order');
 await page.evaluate(()=>showPanel('settings'));assert(await page.getByRole('button',{name:'Export bookmarks',exact:true}).isVisible(),'Bookmark export UI');
 assert(await page.getByRole('button',{name:'Back up profile',exact:true}).isVisible(),'Backup UI');
 await page.screenshot({path:'output/playwright/readiness-settings.png'});
 await websites.close();
 return {passed:['form protection','website IPC isolation','permission prompt and remembered denial','permission reset','media protection','scroll restoration','keyboard tab navigation','backup and export UI','native PDF export','active download protection','renderer crash and recovery']};
}
