async(page)=>{
 const assert=(c,m)=>{if(!c)throw new Error(m);};
 const cmd=(cmd,extra={})=>page.evaluate(({cmd,extra})=>send(cmd,extra),{cmd,extra});
 const until=async(test,label)=>{for(let n=0;n<100;n++){const s=await page.evaluate(()=>state);if(test(s))return s;await page.waitForTimeout(100);}throw new Error('Timeout: '+label);};
 const root='';
 const pass=[];
 await page.evaluate(()=>showPanel('settings'));await page.getByRole('button',{name:'Import browser data',exact:true}).click();
 assert(await page.getByRole('heading',{name:'Import browser data',exact:true}).isVisible(),'Settings entry');
 const before=await page.evaluate(()=>state.bookmarks.length);const alreadyImported=await page.evaluate(()=>state.bookmarks.some(b=>b.url==='http://127.0.0.1:48621/imported'));
 await cmd('import_prepare',{kind:'bookmarks',path:root+'artifacts/import-bookmarks.html'});
 let s=await until(s=>s.importer.status==='preview','bookmark preview');assert(s.importer.counts.bookmarks===1,'HTML dedupe and unsafe URL rejection');assert(s.bookmarks.length===before,'Preview is side effect free');
 await page.getByRole('button',{name:'Cancel',exact:true}).click();await until(s=>s.importer.status==='idle','cancel');
 await cmd('import_apply',{ticket:s.importer.ticket});await page.waitForTimeout(150);assert((await page.evaluate(()=>state.bookmarks.length))===before,'Cancelled preview cannot apply');pass.push('preview and cancel');
 await cmd('import_prepare',{kind:'bookmarks',path:root+'artifacts/import-bookmarks.html'});await until(s=>s.importer.status==='preview','preview again');
 await page.getByRole('button',{name:'Import selected data',exact:true}).click();s=await until(s=>s.importer.status==='done','apply bookmarks');assert(s.importer.bookmarks===(alreadyImported?0:1),'Bookmark added or preserved');
 await cmd('import_prepare',{kind:'bookmarks',path:root+'artifacts/import-bookmarks.html'});await until(s=>s.importer.status==='preview','duplicate preview');await page.getByRole('button',{name:'Import selected data',exact:true}).click();s=await until(s=>s.importer.status==='done','duplicate import');assert(s.importer.bookmarks===0,'Existing bookmark preserved');pass.push('bookmark import and duplicate preservation');
 await cmd('import_prepare',{kind:'cookies',path:root+'artifacts/import-cookies.json'});s=await until(s=>s.importer.status==='preview','cookie preview');assert(s.importer.counts.cookies===2&&s.importer.counts.skipped===2,'cookie validation');assert(!JSON.stringify(s).includes('synthetic-only'),'No plaintext cookies in UI state');
 await page.getByRole('button',{name:'Import selected data',exact:true}).click();s=await until(s=>s.importer.status==='done','cookies written');assert(s.importer.cookies===2&&s.importer.failed===0,'Native cookie manager writes');assert(!(await page.context().cookies()).some(c=>c.name.startsWith('noram_import_')),'Website cookies isolated from Settings');pass.push('native cookie import and privileged UI isolation');
 const sites=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9241');
 try{const context=sites.contexts()[0],cookies=await context.cookies();const http=cookies.find(c=>c.name==='noram_import_http'),secure=cookies.find(c=>c.name==='noram_import_secure');assert(http?.httpOnly&&http.expires===-1&&!http.secure,'HttpOnly session attributes');assert(secure?.httpOnly&&secure.secure&&secure.sameSite==='Strict'&&secure.expires>Date.now()/1000+86400,'Secure persistent attributes');
 await cmd('navigate',{url:'http://127.0.0.1:48621/headers'});let website;for(let n=0;n<100;n++){website=context.pages().find(p=>p.url().includes('/headers'));if(website)break;await page.waitForTimeout(100);}assert(website,'Local cookie request loaded');await website.waitForLoadState('domcontentloaded');assert(await website.evaluate(()=>document.body.innerText.includes('noram_import_http=synthetic-only')),'Imported cookie sent to website');assert(!await website.evaluate(()=>document.cookie.includes('noram_import_http')),'HttpOnly hidden from page script');
 }finally{await sites.close();}pass.push('cookie attributes and real HTTP request');
 await page.evaluate(()=>showPanel('import'));await cmd('import_prepare',{kind:'cookies',path:root+'artifacts/import-bookmarks.html'});await until(s=>s.importer.status==='preview'||s.importer.status==='error','invalid file');
 await cmd('import_prepare',{kind:'cookies',path:root+'artifacts/import-cookies.json'});await until(s=>s.importer.status==='preview','final preview');
 await page.screenshot({path:root+'output/playwright/import-data.png'});
 return {passed:pass};
}
