'use strict';
let state=null,panel='home',lastMessage='',mounted='',editing=null,dragging=null,zoom=1;
const $=id=>document.getElementById(id);
const send=(cmd,extra={})=>window.ipc?.postMessage(JSON.stringify({cmd,...extra}));
const el=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
const active=()=>state?.tabs.find(t=>t.id===state.active);
const engines=[['duckduckgo','DuckDuckGo','!d'],['brave','Brave Search','!b'],['google','Google','!g'],['bing','Bing','!bi'],['startpage','Startpage','!s'],['ecosia','Ecosia','!e'],['wikipedia','Wikipedia','!w']];
const nav=[['home','home','Start page'],['bookmarks','bookmarks','Bookmarks'],['history','history','History'],['tabs','tab','Tabs'],['shields','shield','Privacy & shields'],['performance','rocket','Performance'],['extensions','embed2','Extensions'],['vpn','lock','VPN connections'],['settings','cog','Settings']];
const tabNodes=new Map();
function icon(name){const n=document.createElementNS('http://www.w3.org/2000/svg','svg');n.classList.add('ico');n.setAttribute('aria-hidden','true');const use=document.createElementNS(n.namespaceURI,'use');use.setAttribute('href','/icons.svg#'+name);n.append(use);return n;}
function setIcon(node,name){if(node.dataset.glyph===name)return;node.replaceChildren(icon(name));node.dataset.glyph=name;}
function iconButton(name,label,action){const n=el('button',undefined,'icon-button');n.type='button';n.title=label;n.setAttribute('aria-label',label);n.append(icon(name));n.onclick=action;return n;}
function button(label,name,action,cls='secondary'){const n=el('button',undefined,'button '+cls);n.type='button';if(name)n.append(icon(name));n.append(el('span',label));n.onclick=action;return n;}
function notice(parent,text,name='shield'){const n=el('div',undefined,'notice');n.append(icon(name),el('div',text));parent.append(n);}
function focusAddress(){$('address').focus();$('address').select();}
function navigate(url){panel='home';mounted='';send('navigate',{url});}
function saveSettings(patch){send('settings',{settings:{...state.settings,...patch}});}
function showPanel(next){
  panel=next;mounted='';
  if(next==='home'){navigate('noram://newtab');return;}
  send('panel',{open:true});
  if(next==='extensions')send('extensions');
  if(next==='vpn')send('vpn_refresh');
  if(next==='import')send('import_scan');
  renderPanel();
}
function goBackToPage(){panel='home';mounted='';send('panel',{open:false});renderPanel();}
function selectTab(id){
  if(state){state.active=id;panel='home';mounted='';renderTabs();renderPanel();}
  send('switch',{id});
}
function makeTab(tab){
 const node=el('div',undefined,'tab');node.dataset.id=tab.id;node.setAttribute('role','presentation');node.draggable=true;
 const glyph=el('span',undefined,'tab-icon'),label=el('button',undefined,'tab-label');
 label.type='button';label.setAttribute('role','tab');label.onkeydown=e=>{const keys=['ArrowLeft','ArrowRight','Home','End'];if(!keys.includes(e.key))return;e.preventDefault();const index=state.tabs.findIndex(t=>t.id===tab.id),next=e.key==='Home'?0:e.key==='End'?state.tabs.length-1:(index+(e.key==='ArrowRight'?1:-1)+state.tabs.length)%state.tabs.length;selectTab(state.tabs[next].id);tabNodes.get(state.tabs[next].id)?._label.focus();};label.onpointerdown=e=>{if(e.button===0)selectTab(tab.id);};label.onclick=e=>{if(e.detail===0)selectTab(tab.id);};
 const close=iconButton('cross','Close tab',e=>{e.stopPropagation();send('close',{id:tab.id});});close.classList.add('tab-close');
 node.append(glyph,label,close);node._glyph=glyph;node._label=label;
 node.onauxclick=e=>{if(e.button===1){e.preventDefault();send('close',{id:tab.id});}};
 node.ondblclick=()=>send('pin',{target:tab.id});
 node.oncontextmenu=e=>{e.preventDefault();showPanel('tabs');};
 node.ondragstart=e=>{dragging=tab.id;e.dataTransfer.setData('text/plain',String(tab.id));e.dataTransfer.effectAllowed='move';};
 node.ondragend=()=>{dragging=null;document.querySelectorAll('.drag-over').forEach(n=>n.classList.remove('drag-over'));};
 node.ondragover=e=>{if(dragging&&dragging!==tab.id){e.preventDefault();node.classList.add('drag-over');}};
 node.ondragleave=()=>node.classList.remove('drag-over');
 node.ondrop=e=>{e.preventDefault();node.classList.remove('drag-over');if(!dragging||dragging===tab.id)return;const ids=state.tabs.map(t=>t.id).filter(id=>id!==dragging);let index=ids.indexOf(tab.id);if(e.clientX>node.getBoundingClientRect().left+node.offsetWidth/2)index++;ids.splice(index,0,dragging);send('reorder',{ids});dragging=null;};
 return node;
}
function renderTabs(){
 if(!state)return;const host=$('tabs'),ids=new Set(state.tabs.map(t=>t.id));
 for(const[id,node]of tabNodes){if(!ids.has(id)){node.remove();tabNodes.delete(id);}}
 for(let index=0;index<state.tabs.length;index++){
  const tab=state.tabs[index];let node=tabNodes.get(tab.id);if(!node){node=makeTab(tab);tabNodes.set(tab.id,node);}
  const cls='tab'+(tab.id===state.active?' active':'')+(tab.pinned?' pinned':'')+(tab.sleeping?' sleeping':'')+(tab.loading?' loading':'');
  if(node.className!==cls)node.className=cls;node._label.setAttribute('aria-selected',String(tab.id===state.active));node._label.tabIndex=tab.id===state.active?0:-1;
  if(node._label.textContent!==tab.title)node._label.textContent=tab.title;node._label.title=tab.title+'\n'+tab.url;
  setIcon(node._glyph,tab.loading?'spinner8':tab.muted?'volume-mute2':tab.pinned?'pushpin':tab.sleeping?'cloud':'earth');
  if(host.children[index]!==node)host.insertBefore(node,host.children[index]||null);
 }
}
const systemTheme=matchMedia('(prefers-color-scheme: dark)');
function applyTheme(){if(!state)return;document.documentElement.dataset.theme=state.settings.theme==='system'?(systemTheme.matches?'dark':'light'):state.settings.theme;}
systemTheme.addEventListener('change',applyTheme);
function update(next){
 const previous=state?.active;state=next;
 if(previous!==undefined&&previous!==state.active){panel='home';mounted='';}
 applyTheme();renderTabs();renderPermission();renderUpdateStatus();const tab=active();
 if(document.activeElement!==$('address'))$('address').value=tab?.url==='noram://newtab'?'':tab?.url||'';
 setIcon($('security'),tab?.url.startsWith('https:')?'lock':tab?.url.startsWith('http:')?'earth':'search');
 $('security').title=tab?.url.startsWith('https:')?'HTTPS connection':tab?.url.startsWith('http:')?'Unencrypted HTTP connection':'Search or navigate';
 $('private-badge').hidden=!state.private;
 $('reload').dataset.cmd=tab?.loading?'stop':'reload';setIcon($('reload'),tab?.loading?'cross':'loop2');
 $('star').classList.toggle('saved',state.bookmarks.some(b=>b.url===tab?.url));
 $('bookmark-bar').hidden=!state.settings.bookmarks_bar;document.body.classList.toggle('has-bookmarks',state.settings.bookmarks_bar);
 updateBookmarkBar();renderPanel();updateStats();
 if(state.message&&state.message!==lastMessage){lastMessage=state.message;$('toast-text').textContent=state.message;$('toast').hidden=false;}
}
let bookmarksKey='';
function updateBookmarkBar(){const key=JSON.stringify(state.bookmarks);if(bookmarksKey===key)return;bookmarksKey=key;const host=$('bookmark-bar');host.replaceChildren();for(const item of state.bookmarks.slice(0,25)){const b=el('button');b.append(icon('earth'),el('span',item.title));b.title=item.url;b.onclick=()=>navigate(item.url);host.append(b);}if(!state.bookmarks.length)host.append(el('span','Save your first bookmark with Ctrl+D.','home-note'));}
function updateStats(){for(const n of document.querySelectorAll('[data-stat]')){const key=n.dataset.stat;n.textContent=key==='blocked'?state.blocked.toLocaleString():key==='live'?state.live+' / '+state.settings.max_live_tabs:key==='rules'?state.rules.toLocaleString():'';}}
function pageHeader(parent,title,description){const header=el('div',undefined,'page-header'),text=el('div',undefined,'page-heading');text.append(el('div','YOUR BROWSER','eyebrow'),el('h1',title),el('p',description,'description'));header.append(text,iconButton('cross','Back to website',goBackToPage));parent.append(header);}
function renderPanel(){
 if(!state)return;$('sidebar').hidden=panel==='home';
 if(active()?.crashed&&panel==='home'){const host=$('content');host.replaceChildren();const page=el('div',undefined,'page');pageHeader(page,'This page stopped','Reload to recover this tab. Unsaved website state may have been lost.');page.append(button('Reload tab','loop2',()=>send('retry_tab')));host.append(page);mounted='crashed';return;}

 document.querySelectorAll('.side-link').forEach(n=>n.classList.toggle('selected',n.dataset.panel===panel));
 if(mounted!==panel){mounted=panel;const host=$('content');host.replaceChildren();host.scrollTop=0;
  if(panel==='home')home(host);
  else{const page=el('div',undefined,'page');host.append(page);
   const info={import:['Import browser data','Bring bookmarks, history and cookies into NoRAM.'],bookmarks:['Bookmarks','Keep the useful parts of the web close.'],history:['History',state.private?'This private window records no browsing history.':'Your recent pages, stored on this device.'],tabs:['Your tabs','Find a tab, keep it awake, or make a little room.'],settings:['Settings','Make yourself at home.'],shields:['Privacy & shields','Brave’s adblock engine. EasyList and EasyPrivacy. Controls that mean what they say.'],performance:['Performance','Fewer live pages. Less background work.'],extensions:['Extensions','Load and manage unpacked browser extensions.'],vpn:['VPN connections','Connect using VPN profiles configured in Windows.'],tools:['Browser menu','Everything you need, without the clutter.'],find:['Find in page','Search the current page for text.']};
   const entry=info[panel]||info.tools;pageHeader(page,...entry);
   if(['bookmarks','history','tabs'].includes(panel))collectionPage(page,panel);
   else if(['settings','shields','performance'].includes(panel))settingsPage(page,panel);
   else if(panel==='import')importPage(page);
   else if(panel==='extensions')extensionsPage(page);
   else if(panel==='vpn')vpnPage(page);
   else toolsPage(page,panel==='find');
  }
 }
 if(['bookmarks','history','tabs'].includes(panel))refreshCollection();
 if(panel==='extensions')refreshExtensions();
 if(panel==='vpn')refreshVpn();
 if(panel==='import')refreshImport();
}
function engineSelect(){const select=el('select',undefined,'engine-select');select.setAttribute('aria-label','Search engine');for(const[id,name]of engines){const op=el('option',name);op.value=id;select.append(op);}select.value=state.settings.search_engine;select.onchange=()=>saveSettings({search_engine:select.value});return select;}
function home(parent){
 const page=el('div',undefined,'home'),brand=el('div',undefined,'home-brand');brand.append(el('span','n','brand-mark'),el('span','NoRAM'));page.append(brand,el('h1',state.private?'A little space to yourself.':'A clear start. A wider web.'),el('p',state.private?'Private history stays in this window.':'Find what matters. Leave the noise behind.','subtitle'));
 const form=el('form',undefined,'searchbox'),input=el('input');input.placeholder='Search the web';input.setAttribute('aria-label','Search the web');const submit=iconButton('arrow-right2','Search',()=>{});submit.type='submit';form.append(icon('search'),input,engineSelect(),submit);form.onsubmit=e=>{e.preventDefault();navigate(input.value);};page.append(form,el('div','Ctrl+L for the address bar. Try !g, !b or !w to search elsewhere.','home-hint'));
 const shortcuts=el('div',undefined,'home-shortcuts');for(const[name,url,glyph]of[['Wikipedia','https://wikipedia.org','wikipedia'],['GitHub','https://github.com','github'],['Brave Search','https://search.brave.com','search'],['Bookmarks',null,'bookmarks'],['Settings',null,'cog']]){const b=el('button',undefined,'shortcut'),i=el('span',undefined,'shortcut-icon');i.append(icon(glyph));b.append(i,el('span',name));b.onclick=()=>url?navigate(url):showPanel(name.toLowerCase());shortcuts.append(b);}page.append(shortcuts);
 const overview=el('div',undefined,'home-overview');for(const[glyph,key,label]of[['shield','blocked','Requests blocked'],['tab','live','Live website tabs'],['list','rules','Bundled filter entries']]){const item=el('div',undefined,'overview-item'),text=el('div'),count=el('b','0');count.dataset.stat=key;text.append(count,el('span',label));item.append(icon(glyph),text);overview.append(item);}page.append(overview,el('p',state.private?'Private browsing does not hide your IP or guarantee fingerprint protection.':'Ad and tracker blocking is on by default. Pin tabs with unsaved work before the live-tab budget unloads them.','home-note'));parent.append(page);
}
let collectionKey='',collectionKind='';
function collectionPage(parent,kind){collectionKind=kind;collectionKey='';const filter=el('div',undefined,'search-filter'),input=el('input');input.id='collection-filter';input.placeholder=kind==='tabs'?'Find an open tab':'Search '+kind;input.setAttribute('aria-label',input.placeholder);input.oninput=()=>{collectionKey='';refreshCollection();};filter.append(icon('search'),input);parent.append(filter);
 const actions=el('div',undefined,'actions');if(kind==='history')actions.append(button('Clear history','bin',()=>{if(confirm('Clear browsing history on this device?'))send('clear_history');}));if(kind==='tabs'){actions.append(button('Reopen closed tab','undo2',()=>send('reopen')),button('Close other tabs','cross',()=>send('close_others')),button('Close tabs to the right','arrow-right2',()=>send('close_right')));}parent.append(actions);const rows=el('div',undefined,'collection');rows.id='collection';parent.append(rows);if(kind==='tabs')notice(parent,'Drag tabs along the top to reorder them. Pin tabs to prevent unloading; muted tabs stay muted after they resume.','tab');}
function refreshCollection(){
 const kind=collectionKind,input=$('collection-filter'),rows=$('collection');if(!input||!rows)return;const query=input.value.trim().toLowerCase();let entries=kind==='tabs'?state.tabs:kind==='history'&&state.private?[]:state[kind];
 const key=JSON.stringify([entries,query,kind,state.active]);if(key===collectionKey)return;collectionKey=key;rows.replaceChildren();entries=entries.filter(e=>(e.title+' '+e.url).toLowerCase().includes(query));
 if(!entries.length){rows.append(el('div',query?'No matches. Try another word.':'Nothing here yet.','empty'));return;}
 for(const item of entries){const row=el('div',undefined,'collection-row'+(kind==='tabs'&&item.id===state.active?' tab-current':'')),open=el('button',undefined,'entry-open');open.append(el('b',item.title||item.url),el('small',item.url));open.onclick=()=>kind==='tabs'?selectTab(item.id):navigate(item.url);row.append(icon(kind==='tabs'?(item.pinned?'pushpin':item.sleeping?'cloud':'earth'):kind==='bookmarks'?'bookmark':'history'),open);const actions=el('div',undefined,'row-actions');
  if(kind==='bookmarks'){actions.append(iconButton('cog','Edit bookmark',()=>{editing=item;$('bookmark-name').value=item.title;$('edit-dialog').showModal();}),iconButton('bin','Remove bookmark',()=>send('remove_bookmark',{url:item.url})));}
  else if(kind==='history')actions.append(iconButton('bin','Remove from history',()=>send('remove_history',{url:item.url})));
  else{if(item.crashed)actions.append(el('span','Crashed — select to reload','tag'));if(item.protected||item.protection)actions.append(el('span','Kept awake','tag'));if(item.sleeping)actions.append(el('span','Sleeping','tag'));actions.append(iconButton('pushpin',item.pinned?'Unpin tab':'Pin tab',()=>send('pin',{target:item.id})),iconButton(item.muted?'volume-mute2':'volume-medium',item.muted?'Unmute tab':'Mute tab',()=>send('mute',{target:item.id})),iconButton('copy','Duplicate tab',()=>send('duplicate',{target:item.id})),iconButton('cross','Close tab',()=>send('close',{id:item.id})));}
  row.append(actions);rows.append(row);
 }
}
function setting(parent,title,description,key,type,options){const label=el('label',undefined,'setting'),copy=el('div');copy.append(el('b',title),el('p',description));let control;
 if(type==='select'){control=el('select');for(const[value,text]of options){const op=el('option',text);op.value=value;control.append(op);}control.value=state.settings[key];control.onchange=()=>saveSettings({[key]:key==='max_live_tabs'?Number(control.value):control.value});}
 else{control=el('input');control.type='checkbox';control.checked=state.settings[key];control.onchange=()=>saveSettings({[key]:control.checked});}control.name=key;control.setAttribute('aria-label',title);label.append(copy,control);parent.append(label);
}
function section(parent,title){const s=el('div',undefined,'setting-section');s.append(el('h2',title));parent.append(s);return s;}
function settingsPage(parent,kind){
 if(kind==='settings'){
  const transfer=section(parent,'Your data');transfer.append(button('Import browser data','folder-open',()=>showPanel('import')),button('Export bookmarks','download3',()=>send('export_bookmarks')));
  if(!state.private)transfer.append(button('Back up profile','download3',()=>send('profile_backup')),button('Restore backup on restart','undo2',()=>{if(confirm('Choose a NoRAM JSON backup to restore after restarting. Your current profile will be preserved. Website cookies and passwords are not restored.'))send('profile_restore');}));
  const updates=section(parent,'Updates and installation');setting(updates,'Check for app updates','Check the project’s GitHub releases at startup. Installing an update requires your action and a verified publisher signature.','auto_app_updates');updates.append(button('Check for updates','loop2',()=>send('app_update_check')),button('Install available update','download3',()=>send('app_update_install')),button('Choose default browser','earth',()=>send('default_browser')));const status=el('p',undefined,'description');status.id='update-status';updates.append(status);renderUpdateStatus();
  const permissions=section(parent,'Site permissions');notice(permissions,'Permission prompts identify the requesting origin. Camera and microphone grants keep their tab awake until navigation. Private choices stay in memory.');
  for(const[origin,rules]of Object.entries(state.settings.permissions||{})){const row=el('div',undefined,'collection-row');row.append(el('span',origin+' — '+Object.entries(rules).map(([k,v])=>k+': '+(v?'allowed':'blocked')).join(', ')),button('Reset permissions','undo2',()=>{send('permission_reset',{origin});row.remove();}));permissions.append(row);}

  const appearance=section(parent,'Appearance');setting(appearance,'Theme','Choose a light, dark, or system appearance.','theme','select',[['system','System'],['light','Light'],['dark','Dark']]);setting(appearance,'Bookmarks bar','Keep saved pages below the address bar.','bookmarks_bar');
  const search=section(parent,'Search');setting(search,'Default search engine','Used by the address bar and start page.','search_engine','select',engines.map(([id,name])=>[id,name]));
  const grid=el('div',undefined,'engine-grid');for(const[id,name,bang]of engines){const b=el('button',undefined,'engine-card'+(id===state.settings.search_engine?' selected':''));b.append(el('b',name),el('small',bang+' search terms'));b.onclick=()=>{saveSettings({search_engine:id});grid.querySelectorAll('.engine-card').forEach(n=>n.classList.remove('selected'));b.classList.add('selected');search.querySelector('select').value=id;};grid.append(b);}parent.append(grid);
  const actions=el('div',undefined,'actions');actions.append(button('Privacy controls','shield',()=>showPanel('shields')),button('Performance controls','rocket',()=>showPanel('performance')));parent.append(actions);
  notice(parent,'Typography uses Proxima Nova if installed, with a bundled Figtree fallback. IcoMoon Free icons are credited in the project’s third-party notices.','cog');
 }
 if(kind==='shields'){
  const block=section(parent,'Shields');setting(block,'Block ads and trackers','Brave adblock-rust matches EasyList and EasyPrivacy network rules, including request-type and exception rules.','shields');setting(block,'Hide page ad elements','Apply domain-specific CSS hiding rules after a page loads. Advanced scriptlets and procedural filters are not included.','cosmetic_filtering');setting(block,'Require HTTPS','Upgrade public HTTP pages and block insecure resources. Localhost is exempt.','https_only');setting(block,'Clean tracking parameters','Remove known click identifiers and utm_ values from navigations.','strip_tracking');
  const actions=el('div',undefined,'actions');actions.append(button('Toggle exception for current site','shield',()=>send('exception')),button('Clear cookies and site storage','bin',()=>{if(confirm('Clear cookies, cache and site storage? This signs you out of websites.'))send('clear_data');}));parent.append(actions);
  notice(parent,'Strict engine tracking prevention is active. Supported website permissions ask for your decision. Unknown permission types are denied. Manage saved site permissions in Settings.');
  const exceptions=el('p','Site exceptions: '+(state.settings.exceptions.join(', ')||'none'),'description');parent.append(exceptions);
  setting(parent,'Update filter lists automatically','Check EasyList and EasyPrivacy once a day on normal startup. Failed updates retain the last valid lists.','auto_filter_updates');parent.append(button('Update filters now','loop2',()=>send('filter_update')));const filterStatus=el('p',state.filterStatus,'description');filterStatus.id='filter-status';parent.append(filterStatus);
 }
 if(kind==='performance'){
  const resources=section(parent,'Resource controls');setting(resources,'Maximum live website tabs','The least recently used unpinned background tab unloads when this budget is exceeded.','max_live_tabs','select',[1,2,3,4,6,8,12,16].map(n=>[n,String(n)+' tabs']));
  const actions=el('div',undefined,'actions');actions.append(button(active()?.pinned?'Unpin current tab':'Pin current tab','pushpin',()=>send('pin')),button('Unload current tab','cloud',()=>{if(confirm('Unload this page? This loses page history and unsaved state. Audio, downloads and media-capture tabs cannot be unloaded.'))send('sleep',{confirmed:true});}));parent.append(actions);
  notice(parent,'Automatic unloading protects editable pages, embedded frames, media, active downloads and granted camera/microphone sessions. Failed safety checks keep tabs awake. Protected tabs can exceed the budget. Simple pages resume their saved scroll position; page history is lost. Pin custom web apps whose unsaved state cannot be detected.','warning');
  notice(parent,'The UI keeps tab elements in place, receives native page events, and saves the profile in a background worker. No two-second idle polling. A live-tab limit is not a speed or memory benchmark.','rocket');
 }
}
function extensionsPage(parent){
 const actions=el('div',undefined,'actions');actions.append(button('Load unpacked extension','folder-open',()=>{if(confirm('Extensions can read and change websites. Load this extension only if you trust its code.'))send('extension_add');},'primary'),button('Refresh','loop2',()=>send('extensions')));parent.append(actions);
 const form=el('form',undefined,'path-form'),input=el('input',undefined,'field-input');input.placeholder='Or enter an unpacked extension folder path';input.setAttribute('aria-label','Extension folder path');const submit=button('Load folder','plus',()=>{});submit.type='submit';form.append(input,submit);form.onsubmit=e=>{e.preventDefault();if(input.value.trim()&&confirm('Load this unpacked extension? It can read and change websites.'))send('extension_add',{path:input.value.trim()});};parent.append(form);const list=el('div',undefined,'collection');list.id='extensions-list';parent.append(list);
 notice(parent,state.private?'Extension management is available in normal windows.':'WebView2 supports a subset of Chromium extension APIs. This loads local unpacked extensions; Chrome Web Store installation and extension toolbar popups are not included. Reload open pages after changing an extension.','embed2');
}
let extensionKey='';
function refreshExtensions(){const host=$('extensions-list');if(!host)return;const key=JSON.stringify([state.extensions,state.private]);if(host.dataset.key===key)return;host.dataset.key=key;host.replaceChildren();if(!state.extensions.length)host.append(el('div',state.private?'Use a normal window to manage extensions.':'No extensions installed. Choose a folder containing manifest.json.','empty'));for(const extension of state.extensions){const row=el('div',undefined,'collection-row'),text=el('div',undefined,'entry-open');text.append(el('b',extension.name),el('small',extension.id));row.append(icon('embed2'),text,button(extension.enabled?'Disable':'Enable','power',()=>send('extension_toggle',{id:extension.id,enabled:!extension.enabled})),iconButton('bin','Remove extension',()=>send('extension_remove',{id:extension.id})));host.append(row);}}
function vpnPage(parent){const actions=el('div',undefined,'actions');actions.append(button('Windows VPN settings','cog',()=>send('vpn_settings')),button('Refresh profiles','loop2',()=>send('vpn_refresh')));parent.append(actions);const list=el('div',undefined,'collection');list.id='vpn-list';parent.append(list);notice(parent,'These controls use existing Windows VPN profiles and saved credentials. NoRAM does not provide a VPN server, subscription, browser-only tunnel or kill switch. No profile means there is no VPN connection.','lock');}
function refreshVpn(){const host=$('vpn-list');if(!host)return;const key=JSON.stringify(state.vpn);if(host.dataset.key===key)return;host.dataset.key=key;host.replaceChildren();if(!state.vpn.loaded){host.append(el('div','Reading Windows VPN profiles…','empty'));return;}if(state.vpn.error||!state.vpn.profiles?.length){host.append(el('div',state.vpn.error||'No VPN profiles are configured. Add one in Windows VPN settings.','empty'));return;}for(const profile of state.vpn.profiles){const connected=profile.ConnectionStatus==='Connected',row=el('div',undefined,'collection-row'),text=el('div',undefined,'entry-open');text.append(el('b',profile.Name),el('small',profile.TunnelType+' · '+profile.ConnectionStatus));const action=button(connected?'Disconnect':'Connect','power',()=>{action.disabled=true;send(connected?'vpn_disconnect':'vpn_connect',{name:profile.Name});});row.append(icon('lock'),text,action);host.append(row);}}
function toolsPage(parent,findOnly){
 const form=el('form',undefined,'search-filter'),input=el('input');input.placeholder='Find text on this page';input.setAttribute('aria-label','Find in page');const submit=button('Find next','search',()=>{});submit.type='submit';form.append(icon('search'),input,submit);form.onsubmit=e=>{e.preventDefault();send('find',{text:input.value});goBackToPage();};parent.append(form);if(findOnly){input.focus();return;}
 const grid=el('div',undefined,'tool-grid');for(const[label,glyph,action]of[['New tab','plus',()=>send('new')],['Private window','eye-blocked',()=>send('private')],['Bookmarks','bookmarks',()=>showPanel('bookmarks')],['History','history',()=>showPanel('history')],['Downloads','download3',()=>{send('downloads');goBackToPage();}],['Reopen closed tab','undo2',()=>send('reopen')],['Duplicate tab','copy',()=>send('duplicate')],['Print / save PDF','printer',()=>send('print')],['Save PDF file','download3',()=>send('save_pdf')],['Choose download folder','folder-open',()=>send('download_directory')],['Developer tools','embed2',()=>send('devtools')],['Extensions','embed2',()=>showPanel('extensions')],['VPN connections','lock',()=>showPanel('vpn')],['Settings','cog',()=>showPanel('settings')]]){const b=el('button',undefined,'tool-card');b.append(icon(glyph),el('span',label));b.onclick=action;grid.append(b);}parent.append(grid);
 const actions=el('div',undefined,'actions');for(const[label,glyph,value]of[['Zoom out','shrink',-.1],['Reset zoom','search',0],['Zoom in','enlarge',.1]])actions.append(button(label,glyph,()=>{zoom=value===0?1:Math.max(.25,Math.min(3,zoom+value));send('zoom',{value:zoom});}));parent.append(actions);
 notice(parent,'Ctrl+Shift+T reopens a closed tab. Ctrl+Shift+A searches tabs. Ctrl+H opens history. Ctrl+Shift+O opens bookmarks. Ctrl+F finds text.','keyboard');
}
document.querySelectorAll('[data-icon]').forEach(n=>setIcon(n,n.dataset.icon));
for(const[id,glyph,label]of nav){const b=el('button',undefined,'side-link');b.dataset.panel=id;b.append(icon(glyph),el('span',label));$('side-nav').append(b);}
document.addEventListener('click',e=>{const cmd=e.target.closest('[data-cmd]');if(cmd){send(cmd.dataset.cmd);return;}const n=e.target.closest('[data-panel]');if(n)showPanel(n.dataset.panel);});
$('addressForm').onsubmit=e=>{e.preventDefault();navigate($('address').value);};
$('dismiss-toast').onclick=()=>$('toast').hidden=true;
$('cancel-edit').onclick=()=>$('edit-dialog').close();
$('edit-form').onsubmit=e=>{e.preventDefault();if(editing)send('edit_bookmark',{url:editing.url,title:$('bookmark-name').value});$('edit-dialog').close();};
send('ready');

function importPage(parent){
 parent.classList.add('import-page');
 if(state.private){notice(parent,'Import is unavailable in private windows. Open a regular window to import data.');return;}
 notice(parent,'Preview first, then import. Bookmarks are deduplicated by URL. Up to 2,000 recent history entries are retained. Passwords, payment data, extensions and browser settings are not imported.','shield');
 const profiles=section(parent,'Import from a browser profile');
 const select=el('select',undefined,'engine-select');select.id='import-source';select.setAttribute('aria-label','Source browser profile');profiles.append(select);
 const choices=el('div',undefined,'setting');const options={};for(const[key,label,checked]of[['bookmarks','Bookmarks',true],['history','History',true],['cookies','Firefox cookies',false]]){const wrap=el('label',undefined,'button secondary'),input=el('input');input.type='checkbox';input.checked=checked;input.id='import-'+key;options[key]=input;wrap.append(input,el('span',label));choices.append(wrap);}profiles.append(choices);
 const selection=()=>Object.fromEntries(Object.entries(options).map(([k,v])=>[k,v.checked]));
 profiles.append(button('Preview selected profile','search',()=>{if(select.value!=='')send('import_prepare',{source:Number(select.value),options:selection()});}),button('Choose profile folder','folder-open',()=>send('import_prepare',{kind:'profile',options:selection()})),button('Refresh profiles','loop2',()=>send('import_scan')));
 profiles.append(el('p','For Brave, Chrome, Edge, Vivaldi and Opera, select Bookmarks and/or History. Firefox also supports cookies from its profile folder. Close the source browser if its database is locked.','description'));
 const files=section(parent,'Import exported files');files.append(button('Choose bookmark file','bookmarks',()=>send('import_prepare',{kind:'bookmarks'})),button('Choose cookie file','lock',()=>send('import_prepare',{kind:'cookies'})));
 files.append(el('p','Bookmarks: Netscape HTML or Chromium Bookmarks JSON. Cookies: JSON exports or Netscape cookies.txt (maximum 16 MB, 5,000 cookies). Expired, invalid, partitioned and Firefox container cookies are skipped.','description'));
 notice(files,'Cookie exports contain sensitive login sessions. Only import files you trust. Import replaces cookies with the same name, domain and path. Some sites will still require you to sign in again. Delete the export after use.','lock');
 const help=section(parent,'Export cookies from Brave, Chrome or Edge');help.append(el('p','Encrypted Chromium cookie databases cannot be imported directly. Use the included local cookie exporter: load tools/browser-cookie-export as an unpacked extension in the source browser, open its toolbar action and export a JSON file. Then choose that file above. The helper saves locally and sends nothing over the network.','description'));
 const result=section(parent,'Import preview');result.id='import-result';refreshImport();
}
function refreshImport(){
 const select=$('import-source');if(select){const sources=state.importSources||[],key=JSON.stringify(sources);if(select.dataset.key!==key){select.dataset.key=key;select.replaceChildren();if(!sources.length){const op=el('option','No profiles detected — choose a folder instead');op.value='';select.append(op);}sources.forEach((source,i)=>{const op=el('option',source.name);op.value=i;select.append(op);});}}
 const host=$('import-result');if(!host)return;const data=state.importer||{status:'idle'},key=JSON.stringify(data);if(host.dataset.key===key)return;host.dataset.key=key;host.replaceChildren(el('h2','Import preview'));
 if(data.status==='preview'){const c=data.counts;host.append(el('p',`${c.bookmarks} bookmarks · ${c.history} history entries · ${c.cookies} cookies · ${c.skipped} skipped.`));host.append(el('p','No data has been imported yet. Existing bookmarks are kept. Matching cookies will be replaced.','description'),button('Import selected data','checkmark',()=>send('import_apply',{ticket:data.ticket}),'primary'),button('Cancel','cross',()=>send('import_cancel')));}
 else if(data.status==='reading'){host.append(el('p','Reading data for preview…'),button('Cancel','cross',()=>send('import_cancel')));}
 else if(data.status==='done'){host.append(el('p',`Imported ${data.bookmarks} new bookmarks, ${data.history} new history entries and ${data.cookies} cookies. ${data.skipped} skipped; ${data.failed} cookie writes failed.`));}
 else if(data.status==='error'){notice(host,data.message,'shield');}
 else host.append(el('p','Choose a profile or exported file to preview its contents.','description'));
}

function renderUpdateStatus(){
 const host=$('update-status');if(host)host.textContent=state.updates?.message||({idle:'No update check yet.',checking:'Checking for updates…'}[state.updates?.status]||'');
 const filter=$('filter-status');if(filter)filter.textContent=state.filterStatus;
}
function renderPermission(){
 let dialog=$('permission-dialog');const request=state.permissionRequests?.[0];
 if(!dialog){dialog=el('dialog');dialog.id='permission-dialog';dialog.setAttribute('aria-labelledby','permission-title');const title=el('h2','Website permission');title.id='permission-title';const message=el('p');message.id='permission-message';const actions=el('div',undefined,'actions');for(const[label,allow,remember]of[['Block',false,false],['Allow once',true,false],['Always allow',true,true],['Always block',false,true]])actions.append(button(label,'shield',()=>{const current=state.permissionRequests?.[0];if(current)send('permission_decide',{id:current.id,allow,remember});dialog.close();}));dialog.append(title,message,actions);dialog.oncancel=e=>{e.preventDefault();const current=state.permissionRequests?.[0];if(current)send('permission_decide',{id:current.id,allow:false});dialog.close();};document.body.append(dialog);}
 if(!request){if(dialog.open)dialog.close();return;}
 $('permission-message').textContent=request.origin+' requests '+request.kind+' access.';
 if(!dialog.open)dialog.showModal();
}
