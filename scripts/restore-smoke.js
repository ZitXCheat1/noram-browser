async (page) => {
  const initial=await page.evaluate(()=>({live:state.live,tabs:state.tabs,bookmarks:state.bookmarks}));
  if(initial.live!==0||initial.tabs.filter(t=>t.sleeping).length!==3)throw new Error('Session did not restore lazily');
  if(!initial.bookmarks.some(b=>b.url.includes('/one?q=kept')))throw new Error('Bookmark did not survive restart');
  await page.evaluate(()=>send('switch',{id:1}));
  let content;
  for(let i=0;i<100;i++){content=page.context().pages().find(p=>p.url().includes('48621/two'));if(content)break;await page.waitForTimeout(100);}
  if(!content)throw new Error('Restored page did not resume');
  await content.waitForLoadState('domcontentloaded');
  if(await content.evaluate(()=>localStorage.getItem('noram-qa-normal'))!=='normal-only')throw new Error('Normal storage did not persist');
  await page.evaluate(()=>send('clear_data'));
  await page.waitForFunction(()=>state.message==='Browsing data cleared.');
  if(await content.evaluate(()=>localStorage.getItem('noram-qa-normal'))!==null)throw new Error('Site storage was not cleared');
  await page.evaluate(()=>send('new'));
  return {passed:['lazy session restore with zero live pages','bookmark restoration','normal site storage persistence','native clear-site-data callback and storage removal']};
}
