// Run after smoke.js, with normal and --private windows on the same test profile.
async (page) => {
  const pages=page.context().pages();
  let privateShell;
  for(const p of pages){if(p.url()==='http://noram-ui.localhost/'&&await p.evaluate(()=>state.private))privateShell=p;}
  if(!privateShell)throw new Error('No private NoRAM shell found');
  await privateShell.getByRole('textbox',{name:'Address or search',exact:true}).fill('http://127.0.0.1:48621/private');
  await privateShell.getByRole('textbox',{name:'Address or search',exact:true}).press('Enter');
  let content;
  for(let i=0;i<100;i++){content=page.context().pages().find(p=>p.url().includes('48621/private'));if(content)break;await page.waitForTimeout(100);}
  if(!content)throw new Error('Private content did not load');
  await content.waitForLoadState('domcontentloaded');
  const normalSecret=await content.evaluate(()=>localStorage.getItem('noram-qa-normal'));
  if(normalSecret!==null)throw new Error('Private window can read normal localStorage');
  await content.evaluate(()=>localStorage.setItem('noram-qa-private','private-only'));
  await privateShell.evaluate(()=>send('bookmark'));
  await page.waitForTimeout(300);
  const privateState=await privateShell.evaluate(()=>({private:state.private,bookmarks:state.bookmarks,history:state.history}));
  if(!privateState.private||!privateState.bookmarks.some(b=>b.url.includes('/private')))throw new Error('Private in-memory bookmark failed');
  const normal=page.context().pages().find(p=>p.url().includes('48621/two'));
  if(await normal.evaluate(()=>localStorage.getItem('noram-qa-private'))!==null)throw new Error('Private storage leaked into normal profile');
  return {passed:['private flag','private cannot read normal localStorage','normal cannot read private localStorage','private bookmark remains in memory'],privateHistoryCount:privateState.history.length};
}
