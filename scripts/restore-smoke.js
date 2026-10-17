async (page) => {
  const initial=await page.evaluate(()=>({live:state.live,tabs:state.tabs,bookmarks:state.bookmarks}));
  if(initial.live!==0||initial.tabs.filter(t=>t.sleeping).length!==3)throw new Error('Session did not restore lazily');
  if(!initial.bookmarks.some(b=>b.url.includes('/one?q=kept')))throw new Error('Bookmark did not survive restart');
  const restored=initial.tabs.find(t=>t.url.includes('48621/two'));
  if(!restored||!restored.pinned||!restored.muted)throw new Error('Pinned and muted website state did not restore');
  await page.evaluate(id=>send('switch',{id}),restored.id);
  await page.waitForTimeout(500);
  const contentBrowser=await page.context().browser().browserType().connectOverCDP('http://127.0.0.1:9241');
  let content;
  for(let i=0;i<100;i++){content=contentBrowser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('48621/two'));if(content)break;await page.waitForTimeout(100);}
  if(!content)throw new Error('Restored page did not resume');
  await content.waitForLoadState('domcontentloaded');
  if(await content.evaluate(()=>localStorage.getItem('noram-qa-normal'))!=='normal-only')throw new Error('Normal storage did not persist');
  await page.evaluate(()=>send('clear_data'));
  await page.waitForFunction(()=>state.message==='Browsing data cleared.');
  if(await content.evaluate(()=>localStorage.getItem('noram-qa-normal'))!==null)throw new Error('Site storage was not cleared');
  await page.evaluate(()=>send('new'));
  await contentBrowser.close();
  return {passed:['lazy session restore with zero live pages','bookmark restoration','pin and mute restoration','normal site storage persistence','native clear-site-data callback and storage removal']};
}
