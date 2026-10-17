async(page)=>{
 const assert=(c,m)=>{if(!c)throw new Error(m);};
 await page.evaluate(()=>showPanel('import'));
 assert(await page.getByText('Import is unavailable in private windows. Open a regular window to import data.',{exact:true}).isVisible(),'Private UI blocks import');
 await page.evaluate(()=>send('import_prepare',{kind:'cookies',path:'artifacts/import-cookies.json'}));
 await page.waitForTimeout(200);
 const s=await page.evaluate(()=>state);assert(s.private&&s.importer.status==='idle'&&s.message.includes('unavailable in private'),'Native command rejects private import');
 return {passed:['private UI restriction','native private import rejection']};
}
