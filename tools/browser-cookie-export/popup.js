'use strict';
document.getElementById('export').onclick=async()=>{
 const button=document.getElementById('export'),status=document.getElementById('status');button.disabled=true;
 try{
  const api=globalThis.browser||chrome,domain=document.getElementById('domain').value.trim();
  if(domain&&!/^(?:[a-z0-9-]+\.)*[a-z0-9-]+$/i.test(domain))throw new Error('Enter a domain without a URL or path.');
  const all=await api.cookies.getAll(domain?{domain}:{}),cookies=all.filter(c=>!c.partitionKey);
  const blob=new Blob([JSON.stringify({cookies},null,2)],{type:'application/json'}),url=URL.createObjectURL(blob),link=document.createElement('a');
  link.href=url;link.download='noram-cookies.json';link.click();setTimeout(()=>URL.revokeObjectURL(url),30000);
  status.textContent=`Exported ${cookies.length} cookies. Import the file in NoRAM Settings, then delete it. ${all.length-cookies.length} partitioned cookies excluded.`;
 }catch(e){status.textContent=e.message==='Enter a domain without a URL or path.'?e.message:'Export failed. Check the extension permissions and try again.';}finally{button.disabled=false;}
};
