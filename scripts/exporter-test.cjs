// Tests the exporter with a fake cookie API; never opens a real browser cookie store.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
(async()=>{
 const nodes={export:{disabled:false},status:{textContent:''},domain:{value:'example.com'}};
 let downloaded=false,requested,exported;
 const context={document:{getElementById:id=>nodes[id],createElement:()=>({click(){downloaded=true;}})},chrome:{cookies:{getAll:async query=>{requested=query;return [{name:'sid',value:'synthetic-only',domain:'.example.com',httpOnly:true},{name:'partitioned',partitionKey:{}}];}}},Blob:class{constructor(parts){exported=JSON.parse(parts[0]);}},URL:{createObjectURL:()=> 'blob:fixture',revokeObjectURL:()=>{}},setTimeout:()=>0};
 vm.runInNewContext(fs.readFileSync('tools/browser-cookie-export/popup.js','utf8'),context);
 await nodes.export.onclick();assert(downloaded);assert.equal(requested.domain,'example.com');assert.equal(exported.cookies.length,1);assert(exported.cookies[0].httpOnly);assert(!nodes.status.textContent.includes('synthetic-only'));assert(!nodes.export.disabled);
 nodes.domain.value='https://example.com/path';downloaded=false;await nodes.export.onclick();assert(!downloaded);assert.match(nodes.status.textContent,/without a URL/);
 console.log('Cookie exporter: domain filter, partition exclusion, local export and validation passed.');
})();
