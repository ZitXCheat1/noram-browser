#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod importer;
mod instance;
mod lifecycle;
mod permissions;
mod platform;
mod policy;
mod profile;
mod services;
mod updates;

use anyhow::{Context, Result};
use policy::{address, clean_url, Blocker, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Instant,
};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    NewWindowResponse, Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows,
    WebViewExtWindows,
};

const HOME: &str = "noram://newtab";
const BAR: f64 = 96.;
fn runtime_args(port: Option<u16>) -> String {
    let mut args = "--disable-features=msWebOOUI,msPdfOOUI".to_owned();
    if let Some(port) = port {
        args.push_str(&format!(
            " --remote-debugging-port={port} --remote-debugging-address=127.0.0.1"
        ));
    }
    args
}

#[derive(Debug, Clone)]
enum Action {
    Command(Value),
    Title(u64, String),
    Location(u64, String),
    Loaded(u64),
    New(String),
    ExternalUrl(String, bool),
    Refresh,
    Source(u64, String),
    Counters,
    Quit,
    PermissionPrompt,
    SleepProbe(u64, u64, String),
    Crashed(u64, bool),
    FilterUpdated(std::result::Result<String, String>),
    UpdateStatus(Value),
    Message(String),
    Extensions(Vec<Value>),
    Vpn(Value),
    ImportSources(Vec<importer::Source>),
    ImportPrepared(u64, std::result::Result<importer::Data, String>),
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    title: String,
    url: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct SessionEntry {
    title: String,
    url: String,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    muted: bool,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    settings: Settings,
    bookmarks: Vec<Entry>,
    history: Vec<Entry>,
    session: Vec<SessionEntry>,
}
struct Tab {
    id: u64,
    title: String,
    url: String,
    view: Option<WebView>,
    pinned: bool,
    touched: Instant,
    blocked: Rc<Cell<u64>>,
    loading: bool,
    muted: bool,
    activity: lifecycle::SharedActivity,
    scroll: Option<(f64, f64)>,
    crashed: bool,
    protection: String,
}

struct Browser {
    window: Window,
    shell: Option<WebView>,
    _profile_lock: Option<std::fs::File>,
    tabs: Vec<Tab>,
    active: u64,
    next_id: u64,
    proxy: EventLoopProxy<Action>,
    settings: Rc<RefCell<Settings>>,
    blocker: Rc<RefCell<Blocker>>,
    saved: Saved,
    profile: PathBuf,
    private: bool,
    panel: bool,
    message: String,
    last_render: RefCell<String>,
    closed: Vec<Entry>,
    extensions: Vec<Value>,
    vpn: Value,
    writer: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    writer_thread: Option<std::thread::JoinHandle<()>>,
    content_context: WebContext,
    content_environment:
        Option<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment>,
    extension_host: Option<WebView>,
    debug_port: Option<u16>,
    import_sources: Vec<importer::Source>,
    import_state: Value,
    import_ticket: u64,
    pending_import: Option<importer::Data>,
    permissions: permissions::Queue,
    update_status: Value,
    filter_status: String,
    filter_updating: bool,
    app_updating: bool,
}

impl Browser {
    fn shell(&self) -> &WebView {
        self.shell.as_ref().expect("UI is available until shutdown")
    }
    fn start_filter_update(&mut self) {
        if self.private || self.filter_updating {
            return;
        }
        self.filter_updating = true;
        self.filter_status = "Updating filters…".into();
        let profile = self.profile.clone();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = updates::update_filters(&profile).map_err(|e| e.to_string());
            let _ = proxy.send_event(Action::FilterUpdated(result));
        });
    }
    fn start_app_update(&mut self, install: bool) {
        if self.private || self.app_updating {
            return;
        }
        self.app_updating = true;
        self.update_status = json!({"status":"checking"});
        let proxy = self.proxy.clone();
        let profile = self.profile.clone();
        std::thread::spawn(move || {
            let status = updates::app_update(&profile, install)
                .unwrap_or_else(|e| json!({"status":"error","message":e.to_string()}));
            let _ = proxy.send_event(Action::UpdateStatus(status));
        });
    }

    fn ensure_extension_host(&mut self) -> Result<()> {
        if self.extension_host.is_some() {
            return Ok(());
        }
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.content_context)
            .with_browser_extensions_enabled(true)
            .with_incognito(self.private)
            .with_visible(false)
            .with_bounds(Rect {
                position: LogicalPosition::new(0., 0.).into(),
                size: LogicalSize::new(1., 1.).into(),
            })
            .with_navigation_handler(|url| url == "about:blank")
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_additional_browser_args(runtime_args(self.debug_port.map(|p| p + 1)));
        if let Some(environment) = &self.content_environment {
            builder = builder.with_environment(environment.clone());
        }
        let host = builder.build_as_child(&self.window)?;
        self.content_environment = Some(host.environment());
        self.extension_host = Some(host);
        Ok(())
    }
    fn bounds(&self) -> Rect {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        Rect {
            position: LogicalPosition::new(0., self.bar_height()).into(),
            size: LogicalSize::new(
                size.width.max(1.),
                (size.height - self.bar_height()).max(1.),
            )
            .into(),
        }
    }
    fn bar_height(&self) -> f64 {
        BAR + if self.settings.borrow().bookmarks_bar {
            28.
        } else {
            0.
        }
    }
    fn save(&mut self) -> Result<()> {
        if self.private {
            return Ok(());
        }
        self.saved.settings = self.settings.borrow().clone();
        self.saved.session = self
            .tabs
            .iter()
            .map(|t| SessionEntry {
                title: t.title.clone(),
                url: t.url.clone(),
                pinned: t.pinned,
                muted: t.muted,
            })
            .collect();
        if let Some(writer) = &self.writer {
            writer.send(serde_json::to_vec_pretty(&self.saved)?)?;
        }
        Ok(())
    }
    fn persist(&mut self) {
        if let Err(e) = self.save() {
            self.message = format!("Could not save profile: {e}");
        }
    }
    fn render(&self) {
        let data = json!({
            "tabs": self.tabs.iter().map(|t| json!({"id":t.id,"title":t.title,"url":t.url,"sleeping": t.view.is_none() && t.url != HOME,"pinned":t.pinned,"loading":t.loading,"muted":t.muted,"protected":t.activity.protected(),"protection":t.protection,"crashed":t.crashed})).collect::<Vec<_>>(),
            "active":self.active,"private":self.private,"settings":*self.settings.borrow(),
            "bookmarks":self.saved.bookmarks,"history":self.saved.history.iter().rev().take(150).collect::<Vec<_>>(),
            "blocked":self.tabs.iter().map(|t| t.blocked.get()).sum::<u64>(),
            "live":self.tabs.iter().filter(|t|t.view.is_some()).count(),"rules":self.blocker.borrow().len(),"message":self.message,
            "closedCount":self.closed.len(),"extensions":self.extensions,"vpn":self.vpn,
            "importSources":self.import_sources,"importer":self.import_state,
            "permissionRequests":self.permissions.borrow().iter().map(|r| json!({"id":r.id,"tab":r.tab,"origin":r.origin,"kind":r.kind})).collect::<Vec<_>>(),
            "updates":self.update_status,"filterStatus":self.filter_status,"filterUpdating":self.filter_updating
        });
        let payload = data.to_string();
        if *self.last_render.borrow() == payload {
            return;
        }
        *self.last_render.borrow_mut() = payload;
        let _ = self
            .shell()
            .evaluate_script(&format!("window.update({data})"));
    }
    fn visibility(&self) {
        for t in &self.tabs {
            if let Some(v) = &t.view {
                let _ = v.set_visible(t.id == self.active && !self.panel);
            }
        }
    }
    fn add(&mut self, url: String) -> Result<()> {
        anyhow::ensure!(
            self.tabs.len() < 128,
            "Tab limit reached (128). Close a tab first."
        );
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            title: "New tab".into(),
            url,
            view: None,
            pinned: false,
            touched: Instant::now(),
            blocked: Rc::new(Cell::new(0)),
            loading: false,
            muted: false,
            activity: Default::default(),
            scroll: None,
            crashed: false,
            protection: String::new(),
        });
        self.activate(id)
    }
    fn activate(&mut self, id: u64) -> Result<()> {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
            return Ok(());
        };
        self.active = id;
        self.panel = false;
        self.tabs[i].touched = Instant::now();
        self.tabs[i].protection.clear();
        self.visibility();
        if self.tabs[i].url != HOME && self.tabs[i].view.is_none() {
            self.wake(i)?;
        }
        self.visibility();
        self.enforce_budget();
        self.persist();
        Ok(())
    }
    fn enforce_budget(&mut self) {
        let budget = self.settings.borrow().max_live_tabs;
        if self.tabs.iter().any(|t| t.activity.probing.get()) {
            return;
        }
        if self.tabs.iter().filter(|t| t.view.is_some()).count() <= budget {
            return;
        }
        let mut candidates: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| {
                t.id != self.active
                    && !t.pinned
                    && t.view.is_some()
                    && !t.loading
                    && !t.activity.protected()
                    && !t.activity.probing.get()
                    && t.protection.is_empty()
            })
            .collect();
        candidates.sort_by_key(|t| t.touched);
        for t in candidates.into_iter().take(1) {
            t.activity.probing.set(true);
            if platform::probe(
                t.view.as_ref().unwrap(),
                t.id,
                t.activity.generation.get(),
                self.proxy.clone(),
            )
            .is_err()
            {
                t.activity.probing.set(false);
            }
        }
    }
    fn wake(&mut self, i: usize) -> Result<()> {
        let id = self.tabs[i].id;
        let proxy = self.proxy.clone();
        let titles = self.proxy.clone();
        let loads = self.proxy.clone();
        let settings = self.settings.clone();
        let nav_proxy = self.proxy.clone();
        let bounds = self.bounds();
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.content_context)
            .with_browser_extensions_enabled(true)
            .with_additional_browser_args(runtime_args(self.debug_port.map(|p| p + 1)));
        if let Some(environment) = &self.content_environment {
            builder = builder.with_environment(environment.clone());
        }
        let view = builder
            .with_autoplay(false)
            .with_bounds(bounds)
            .with_visible(false)
            .with_incognito(self.private)
            .with_devtools(true)
            .with_clipboard(false)
            .with_navigation_handler(move |url| {
                if url == "about:blank" {
                    return true;
                }
                let Ok(clean) = clean_url(&url, &settings.borrow()) else {
                    return false;
                };
                if clean != url {
                    let _ = nav_proxy.send_event(Action::Command(
                        json!({"cmd":"redirect","id":id,"url":clean}),
                    ));
                    return false;
                }
                let _ = proxy.send_event(Action::Location(id, url));
                true
            })
            .with_document_title_changed_handler(move |title| {
                let _ = titles.send_event(Action::Title(id, title));
            })
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, wry::PageLoadEvent::Finished) {
                    let _ = loads.send_event(Action::Loaded(id));
                }
            })
            .build_as_child(&self.window)?;
        // Install network and permission policy before the first untrusted navigation.
        platform::configure(
            &view,
            self.settings.clone(),
            self.blocker.clone(),
            self.tabs[i].blocked.clone(),
            self.proxy.clone(),
            id,
            platform::TabContext {
                activity: self.tabs[i].activity.clone(),
                permissions: self.permissions.clone(),
            },
        )?;
        view.load_url(&self.tabs[i].url)?;
        self.content_environment = Some(view.environment());
        self.tabs[i].view = Some(view);
        self.tabs[i].crashed = false;
        if self.tabs[i].muted {
            platform::mute(self.tabs[i].view.as_ref().unwrap(), true)?;
        }
        self.tabs[i].loading = true;
        Ok(())
    }
    fn navigate(&mut self, input: &str) -> Result<()> {
        let url = address(input, &self.settings.borrow()).map_err(anyhow::Error::msg)?;
        let i = self
            .tabs
            .iter()
            .position(|t| t.id == self.active)
            .context("No active tab")?;
        self.panel = false;
        self.tabs[i].scroll = None;
        self.tabs[i].url = url.clone();
        if url == HOME {
            self.tabs[i].view.take();
            self.tabs[i].title = "New tab".into();
            self.tabs[i].loading = false;
        } else if let Some(view) = &self.tabs[i].view {
            view.load_url(&url)?;
            self.tabs[i].loading = true;
        } else {
            self.wake(i)?;
        }
        self.enforce_budget();
        self.visibility();
        self.persist();
        Ok(())
    }
    fn command(&mut self, value: Value) -> Result<()> {
        let cmd = value["cmd"].as_str().unwrap_or("");
        let i = self
            .tabs
            .iter()
            .position(|t| t.id == value["target"].as_u64().unwrap_or(self.active))
            .context("Tab was not found")?;
        match cmd {
            "quit" => {
                let _ = self.proxy.send_event(Action::Quit);
            }
            "permission_decide" => {
                let id = value["id"].as_u64().context("Missing permission request")?;
                let request = {
                    let mut queue = self.permissions.borrow_mut();
                    queue
                        .iter()
                        .position(|r| r.id == id)
                        .map(|i| queue.remove(i))
                };
                if let Some(request) = request {
                    let valid = self.tabs.iter().any(|t| {
                        t.id == request.tab
                            && t.view.is_some()
                            && t.activity.generation.get() == request.generation
                    });
                    let allow = valid && value["allow"].as_bool() == Some(true);
                    unsafe {
                        request.args.SetState(if allow { webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PERMISSION_STATE_ALLOW } else { webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PERMISSION_STATE_DENY })?;
                    }
                    if allow && matches!(request.kind.as_str(), "camera" | "microphone") {
                        request.activity.capture.set(true);
                    }
                    if valid && value["remember"].as_bool() == Some(true) {
                        self.settings
                            .borrow_mut()
                            .permissions
                            .entry(request.origin.clone())
                            .or_default()
                            .insert(request.kind.clone(), allow);
                        self.persist();
                    }
                }
                if self.permissions.borrow().is_empty() {
                    self.panel = false;
                    self.visibility();
                }
            }
            "permission_reset" => {
                let origin = value["origin"].as_str().context("Missing origin")?;
                self.settings.borrow_mut().permissions.remove(origin);
                self.persist();
                self.message =
                    "Site permissions reset. Reload the site to stop existing media sessions."
                        .into();
            }
            "filter_update" => {
                self.start_filter_update();
            }
            "app_update_check" => {
                self.start_app_update(false);
            }
            "app_update_install" => {
                self.start_app_update(true);
            }
            "export_bookmarks" => {
                if let Some(path) = services::save_file("Export bookmarks", "bookmarks.html")? {
                    profile::export_bookmarks(&path, &self.saved.bookmarks)?;
                    self.message = "Bookmarks exported.".into();
                }
            }
            "profile_backup" => {
                anyhow::ensure!(!self.private, "Backups are unavailable in private windows");
                if let Some(path) = services::save_file(
                    "Back up NoRAM preferences and bookmarks",
                    "noram-backup.json",
                )? {
                    profile::write_backup(&path, &self.saved)?;
                    self.message = "Preferences, bookmarks, history and tab URLs backed up. Website data is separate.".into();
                }
            }
            "profile_restore" => {
                anyhow::ensure!(!self.private, "Restore is unavailable in private windows");
                if let Some(source) = services::pick_import_file()? {
                    profile::stage_restore(&self.profile, &source)?;
                    self.message = "Backup validated. Close and reopen NoRAM to restore it; your current profile will be preserved.".into();
                }
            }
            "default_browser" => {
                services::default_browser()?;
            }
            "retry_tab" => {
                anyhow::ensure!(self.tabs[i].view.is_none(), "This tab is already running");
                self.wake(i)?;
                self.visibility();
            }
            "import_scan" => {
                anyhow::ensure!(!self.private, "Import is unavailable in private windows");
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let _ = proxy.send_event(Action::ImportSources(importer::discover()));
                });
            }
            "import_cancel" => {
                self.import_ticket += 1;
                self.pending_import = None;
                self.import_state = json!({"status":"idle"});
            }
            "import_prepare" => {
                anyhow::ensure!(!self.private, "Import is unavailable in private windows");
                let kind = value["kind"].as_str().unwrap_or("profile").to_owned();
                let path = if let Some(index) = value["source"].as_u64() {
                    self.import_sources
                        .get(index as usize)
                        .context("Select a browser profile")?
                        .path
                        .clone()
                } else if let Some(path) = value["path"].as_str() {
                    PathBuf::from(path)
                } else {
                    let selected = if kind == "profile" {
                        services::pick_import_folder()?
                    } else {
                        services::pick_import_file()?
                    };
                    let Some(path) = selected else { return Ok(()) };
                    path
                };
                let options = serde_json::from_value::<importer::Options>(value["options"].clone())
                    .unwrap_or_default();
                self.import_ticket += 1;
                let ticket = self.import_ticket;
                self.pending_import = None;
                self.import_state = json!({"status":"reading"});
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let result = if kind == "profile" {
                        importer::profile(&path, &options)
                    } else {
                        importer::file(&path, &kind)
                    };
                    // Parser errors may include source text; never expose cookie values.
                    let result=result.map_err(|_|"Could not read this import. Check the format, selected data and profile folder. Close the source browser if its database is locked. Chromium cookies require a JSON or Netscape export file.".to_owned());
                    let _ = proxy.send_event(Action::ImportPrepared(ticket, result));
                });
            }
            "import_apply" => {
                anyhow::ensure!(!self.private, "Import is unavailable in private windows");
                anyhow::ensure!(
                    value["ticket"].as_u64() == Some(self.import_ticket),
                    "Import preview expired; preview again"
                );
                anyhow::ensure!(
                    self.pending_import.is_some(),
                    "Preview data before importing"
                );
                if !self.pending_import.as_ref().unwrap().cookies.is_empty() {
                    self.ensure_extension_host()?;
                }
                let data = self.pending_import.take().unwrap();
                let (cookies, failed) = if data.cookies.is_empty() {
                    (0, 0)
                } else {
                    platform::import_cookies(self.extension_host.as_ref().unwrap(), &data.cookies)?
                };
                let bookmarks = importer::merge(&mut self.saved.bookmarks, data.bookmarks);
                let history = importer::merge(&mut self.saved.history, data.history);
                if self.saved.history.len() > 2000 {
                    self.saved.history.drain(..self.saved.history.len() - 2000);
                }
                self.import_state = json!({"status":"done","bookmarks":bookmarks,"history":history,"cookies":cookies,"failed":failed,"skipped":data.skipped});
                self.persist();
                self.message="Import complete. Existing matching cookies were replaced; other browsing data was preserved.".into();
            }
            "ready" => {}
            "reopen" => {
                if let Some(entry) = self.closed.pop() {
                    self.add(entry.url)?;
                }
            }
            "duplicate" => self.add(self.tabs[i].url.clone())?,
            "mute" => {
                let tab = &mut self.tabs[i];
                tab.muted = !tab.muted;
                if let Some(v) = &tab.view {
                    platform::mute(v, tab.muted)?;
                }
                self.persist();
            }
            "reorder" => {
                let ids: Vec<u64> = serde_json::from_value(value["ids"].clone())?;
                anyhow::ensure!(
                    ids.len() == self.tabs.len()
                        && ids.iter().collect::<std::collections::HashSet<_>>().len() == ids.len()
                        && ids.iter().all(|id| self.tabs.iter().any(|t| t.id == *id)),
                    "Invalid tab order"
                );
                self.tabs
                    .sort_by_key(|t| ids.iter().position(|id| *id == t.id));
                self.persist();
            }
            "close_others" | "close_right" => {
                let ids: Vec<_> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(index, t)| {
                        !t.pinned && t.id != self.active && (cmd == "close_others" || *index > i)
                    })
                    .map(|(_, t)| t.id)
                    .collect();
                for id in ids {
                    self.command(json!({"cmd":"close","id":id}))?;
                }
            }
            "save_pdf" => {
                let path = if let Some(path) = value["path"].as_str() {
                    Some(PathBuf::from(path))
                } else {
                    services::save_file("Save page as PDF", "page.pdf")?
                };
                if let (Some(view), Some(path)) = (&self.tabs[i].view, path) {
                    platform::save_pdf(view, &path, self.proxy.clone())?;
                }
            }
            "download_directory" => {
                let path = if let Some(path) = value["path"].as_str() {
                    Some(PathBuf::from(path))
                } else {
                    services::pick_folder()?
                };
                if let Some(path) = path {
                    if let Some(view) = &self.tabs[i].view {
                        platform::download_directory(view, &path)?;
                        self.message = "Download folder updated.".into();
                    }
                }
            }
            "downloads" => {
                if let Some(v) = &self.tabs[i].view {
                    platform::downloads(v)?;
                } else {
                    self.message = "Open a website tab to view its downloads.".into();
                }
            }
            "extensions" => {
                if !self.private {
                    self.ensure_extension_host()?;
                    platform::extensions(
                        self.extension_host.as_ref().unwrap(),
                        self.proxy.clone(),
                    )?;
                }
            }
            "extension_add" => {
                anyhow::ensure!(!self.private, "Manage extensions in a normal window");
                let path = if let Some(path) = value["path"].as_str() {
                    Some(PathBuf::from(path))
                } else {
                    services::pick_folder()?
                };
                if let Some(path) = path {
                    self.ensure_extension_host()?;
                    platform::add_extension(
                        self.extension_host.as_ref().unwrap(),
                        &path,
                        self.proxy.clone(),
                    )?;
                }
            }
            "extension_toggle" | "extension_remove" => {
                anyhow::ensure!(!self.private, "Manage extensions in a normal window");
                self.ensure_extension_host()?;
                platform::change_extension(
                    self.extension_host.as_ref().unwrap(),
                    value["id"].as_str().unwrap_or(""),
                    if cmd == "extension_remove" {
                        None
                    } else {
                        Some(value["enabled"].as_bool().unwrap_or(false))
                    },
                    self.proxy.clone(),
                )?;
            }
            "vpn_refresh" => services::vpn_refresh(self.proxy.clone()),
            "vpn_connect" | "vpn_disconnect" => {
                let name = value["name"].as_str().unwrap_or("");
                anyhow::ensure!(
                    self.vpn["profiles"]
                        .as_array()
                        .is_some_and(|profiles| profiles
                            .iter()
                            .any(|p| p["Name"].as_str() == Some(name))),
                    "Select an existing VPN profile first"
                );
                anyhow::ensure!(
                    !name.starts_with(['-', '/']),
                    "Rename this VPN profile in Windows before using it here."
                );
                self.vpn["loaded"] = json!(false);
                services::vpn_action(name.to_owned(), cmd == "vpn_connect", self.proxy.clone());
            }
            "vpn_settings" => services::vpn_settings()?,
            "navigate" => self.navigate(value["url"].as_str().unwrap_or(""))?,
            "new" => self.add(HOME.into())?,
            "switch" => self.activate(value["id"].as_u64().unwrap_or(0))?,
            "close" => {
                let id = value["id"].as_u64().unwrap_or(self.active);
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                    let t = &self.tabs[index];
                    self.closed.push(Entry {
                        title: t.title.clone(),
                        url: t.url.clone(),
                    });
                    if self.closed.len() > 20 {
                        self.closed.remove(0);
                    }
                    self.permissions.borrow_mut().retain(|r| r.tab != id);
                    self.tabs.remove(index);
                    if self.tabs.is_empty() {
                        self.add(HOME.into())?;
                    } else if id == self.active {
                        let next = self.tabs[index.min(self.tabs.len() - 1)].id;
                        self.activate(next)?;
                    }
                    self.persist();
                }
            }
            "redirect" => {
                let id = value["id"].as_u64().unwrap_or(0);
                if let Some(tab) = self.tabs.iter().find(|t| t.id == id) {
                    if let Some(view) = &tab.view {
                        view.load_url(value["url"].as_str().unwrap_or("about:blank"))?;
                    }
                }
            }
            "back" | "forward" | "reload" | "stop" | "print" | "devtools" | "zoom" | "find" => {
                if let Some(view) = &self.tabs[i].view {
                    match cmd {
                        "back" => platform::back(view)?,
                        "forward" => platform::forward(view)?,
                        "reload" => view.reload()?,
                        "stop" => platform::stop(view)?,
                        "print" => {
                            view.evaluate_script("window.print()")?;
                        }
                        "devtools" => view.open_devtools(),
                        "zoom" => {
                            view.zoom(value["value"].as_f64().unwrap_or(1.).clamp(0.25, 3.))?
                        }
                        "find" => {
                            view.evaluate_script(&format!(
                                "window.find({},false,false,true)",
                                serde_json::to_string(value["text"].as_str().unwrap_or(""))?
                            ))?;
                        }
                        _ => {}
                    }
                }
            }
            "pin" => {
                self.tabs[i].pinned = !self.tabs[i].pinned;
                self.persist();
            }
            "sleep" => {
                if !self.tabs[i].pinned
                    && !self.tabs[i].activity.protected()
                    && value["confirmed"].as_bool() == Some(true)
                {
                    self.tabs[i].view.take();
                    self.tabs[i].loading = false;
                    self.add(HOME.into())?;
                } else {
                    self.message = "Unloading needs confirmation and is unavailable during audio, downloads or media capture.".into();
                }
            }
            "bookmark" => {
                let t = &self.tabs[i];
                if t.url != HOME {
                    if let Some(index) = self.saved.bookmarks.iter().position(|b| b.url == t.url) {
                        self.saved.bookmarks.remove(index);
                    } else {
                        self.saved.bookmarks.push(Entry {
                            title: t.title.clone(),
                            url: t.url.clone(),
                        });
                    }
                    self.persist();
                }
            }
            "remove_bookmark" => {
                self.saved
                    .bookmarks
                    .retain(|b| Some(b.url.as_str()) != value["url"].as_str());
                self.persist();
            }
            "panel" => {
                self.panel = value["open"].as_bool().unwrap_or(false);
                self.visibility();
            }
            "edit_bookmark" => {
                if let Some(bookmark) = self
                    .saved
                    .bookmarks
                    .iter_mut()
                    .find(|b| Some(b.url.as_str()) == value["url"].as_str())
                {
                    bookmark.title = value["title"]
                        .as_str()
                        .unwrap_or(&bookmark.title)
                        .chars()
                        .take(200)
                        .collect();
                    self.persist();
                }
            }
            "remove_history" => {
                self.saved
                    .history
                    .retain(|h| Some(h.url.as_str()) != value["url"].as_str());
                self.persist();
            }
            "settings" => {
                let previous = self.settings.borrow().clone();
                let mut settings: Settings = serde_json::from_value(value["settings"].clone())?;
                settings.max_live_tabs = settings.max_live_tabs.clamp(1, 16);
                anyhow::ensure!(
                    [
                        "duckduckgo",
                        "brave",
                        "google",
                        "bing",
                        "startpage",
                        "ecosia",
                        "wikipedia"
                    ]
                    .contains(&settings.search_engine.as_str()),
                    "Unknown search engine"
                );
                anyhow::ensure!(
                    ["system", "light", "dark"].contains(&settings.theme.as_str()),
                    "Unknown theme"
                );
                settings.exceptions = previous.exceptions;
                settings.permissions = previous.permissions;
                *self.settings.borrow_mut() = settings;
                self.enforce_budget();
                for tab in &self.tabs {
                    if let Some(v) = &tab.view {
                        v.set_bounds(self.bounds())?;
                    }
                }
                self.persist();
                self.message =
                    "Policy saved. Reload open pages to apply it to existing resources.".into();
            }
            "exception" => {
                if let Ok(url) = url::Url::parse(&self.tabs[i].url) {
                    if let Some(host) = url.host_str() {
                        let mut settings = self.settings.borrow_mut();
                        if !settings.exceptions.remove(host) {
                            settings.exceptions.insert(host.to_owned());
                        }
                    }
                }
                self.persist();
                self.message = "Site shield exception updated. Reload this tab.".into();
            }
            "clear_history" => {
                self.saved.history.clear();
                self.persist();
            }
            "clear_data" => {
                self.ensure_extension_host()?;
                platform::clear_data(self.extension_host.as_ref().unwrap(), self.proxy.clone())?;
            }
            "private" => {
                std::process::Command::new(std::env::current_exe()?)
                    .arg("--private")
                    .arg("--profile")
                    .arg(&self.profile)
                    .spawn()?;
            }
            "focus" => {
                self.shell().focus()?;
                self.shell().evaluate_script("focusAddress()")?;
            }
            "next" => {
                let next = (i + 1) % self.tabs.len();
                self.activate(self.tabs[next].id)?;
            }
            "previous" => {
                let next = (i + self.tabs.len() - 1) % self.tabs.len();
                self.activate(self.tabs[next].id)?;
            }
            "show_bookmarks" | "show_history" | "show_settings" | "show_tabs" | "show_find" => {
                self.panel = true;
                self.visibility();
                self.shell().focus()?;
                let target = match cmd {
                    "show_bookmarks" => "bookmarks",
                    "show_history" => "history",
                    "show_settings" => "settings",
                    "show_tabs" => "tabs",
                    _ => "find",
                };
                self.shell()
                    .evaluate_script(&format!("showPanel({})", serde_json::to_string(target)?))?;
            }
            _ => {}
        }
        Ok(())
    }
    fn event(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Command(v) => self.command(v)?,
            Action::ExternalUrl(url, foreground) => {
                if foreground {
                    self.window.set_visible(true);
                }
                let url = address(&url, &self.settings.borrow()).map_err(anyhow::Error::msg)?;
                self.add(url)?;
                self.shell().focus()?;
            }
            Action::New(url) => {
                let url = address(&url, &self.settings.borrow()).map_err(anyhow::Error::msg)?;
                self.add(url)?;
            }
            Action::Title(id, title) => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.title = title;
                }
            }
            Action::Location(id, url) => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.url = url;
                    t.activity
                        .generation
                        .set(t.activity.generation.get().wrapping_add(1));
                    t.activity.capture.set(false);
                    t.protection.clear();
                    self.permissions.borrow_mut().retain(|r| r.tab != id);
                    t.loading = true;
                }
            }
            Action::Source(id, url) => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.url = url;
                }
            }
            Action::Counters | Action::Quit => {}
            Action::PermissionPrompt => {
                self.panel = true;
                self.visibility();
                self.shell().focus()?;
            }
            Action::SleepProbe(id, generation, raw) => {
                let budget = self.settings.borrow().max_live_tabs;
                let live = self.tabs.iter().filter(|t| t.view.is_some()).count();
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.activity.probing.set(false);
                    if t.activity.generation.get() == generation
                        && t.id != self.active
                        && !t.pinned
                        && !t.loading
                        && !t.activity.protected()
                        && t.view.is_some()
                        && live > budget
                    {
                        if let Some(scroll) = lifecycle::snapshot(&raw, &t.url) {
                            t.scroll = Some(scroll);
                            t.view.take();
                            t.loading = false;
                            t.protection.clear();
                        } else {
                            t.protection = "Editable content, embedded content, media or an unavailable safety check".into();
                        }
                    }
                }
                self.enforce_budget();
            }
            Action::Crashed(id, whole_environment) => {
                self.permissions
                    .borrow_mut()
                    .retain(|r| !whole_environment && r.tab != id);
                if whole_environment {
                    self.extension_host.take();
                    self.content_environment.take();
                }
                for t in &mut self.tabs {
                    if t.id == id || whole_environment {
                        t.view.take();
                        t.loading = false;
                        t.crashed = true;
                        t.activity = Default::default();
                    }
                }
                self.message = "A website process stopped. Select its tab to reload. Unsaved page state may be lost.".into();
            }
            Action::FilterUpdated(result) => {
                self.filter_updating = false;
                if result.is_ok() {
                    *self.blocker.borrow_mut() = Blocker::load(&self.profile, self.private);
                }
                self.filter_status = match result {
                    Ok(message) => message,
                    Err(e) => format!("Filter update failed; existing lists retained: {e}"),
                };
                self.message = self.filter_status.clone();
            }
            Action::UpdateStatus(status) => {
                self.app_updating = false;
                self.update_status = status;
            }

            Action::Message(message) => self.message = message,
            Action::Extensions(extensions) => self.extensions = extensions,
            Action::Vpn(vpn) => self.vpn = vpn,
            Action::ImportSources(sources) => self.import_sources = sources,
            Action::ImportPrepared(ticket, result) => {
                if ticket == self.import_ticket && !self.private {
                    match result {
                        Ok(data) => {
                            self.import_state =
                                json!({"status":"preview","ticket":ticket,"counts":data.summary()});
                            self.pending_import = Some(data);
                        }
                        Err(message) => {
                            self.import_state = json!({"status":"error","message":message})
                        }
                    }
                }
            }
            Action::Loaded(id) => {
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    if let (Some((x, y)), Some(view)) = (t.scroll.take(), t.view.as_ref()) {
                        let _ = view.evaluate_script(&format!("scrollTo({x},{y})"));
                    }
                }
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                    t.loading = false;
                    // Read final URL from the engine, including same-document history changes.
                    if let Some(view) = &t.view {
                        if let Ok(url) = view.url() {
                            if url.starts_with("http") {
                                t.url = url;
                            }
                        }
                    }
                    if !self.private && t.url.starts_with("http") {
                        if self.saved.history.last().is_none_or(|h| h.url != t.url) {
                            self.saved.history.push(Entry {
                                title: t.title.clone(),
                                url: t.url.clone(),
                            });
                        }
                        if self.saved.history.len() > 2000 {
                            self.saved.history.drain(..self.saved.history.len() - 2000);
                        }
                    }
                }
                if let Some(t) = self.tabs.iter().find(|t| t.id == id) {
                    let settings = self.settings.borrow();
                    let except = url::Url::parse(&t.url)
                        .ok()
                        .and_then(|u| u.host_str().map(str::to_owned))
                        .is_some_and(|h| {
                            settings
                                .exceptions
                                .iter()
                                .any(|r| policy::host_matches(&h, r))
                        });
                    if settings.shields && settings.cosmetic_filtering && !except {
                        if let Some(v) = &t.view {
                            v.evaluate_script(&self.blocker.borrow().cosmetic_script(&t.url))?;
                        }
                    }
                }
                self.enforce_budget();
                self.persist();
            }
            Action::Refresh => {
                self.message = "Browsing data cleared.".into();
            }
        }
        self.render();
        Ok(())
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let private = args.iter().any(|a| a == "--private");
    let debug_port = if let Some(i) = args.iter().position(|a| a == "--debug-port") {
        let port: u16 = args
            .get(i + 1)
            .context("--debug-port needs a loopback port")?
            .parse()?;
        anyhow::ensure!(
            (1024..65534).contains(&port),
            "Debug port must be between 1024 and 65533"
        );
        Some(port)
    } else {
        None
    };
    let profile = if let Some(i) = args.iter().position(|a| a == "--profile") {
        PathBuf::from(args.get(i + 1).context("--profile needs a directory")?)
    } else {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is missing")?)
            .join("NoRamBrowser")
    };
    std::fs::create_dir_all(&profile)?;
    let profile_lock = if !private {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(profile.join("browser.lock"))?;
        if let Err(error) = lock.try_lock() {
            if let Some(url) = args
                .iter()
                .find(|a| a.starts_with("https://") || a.starts_with("http://"))
            {
                instance::forward(&profile, url, !args.iter().any(|a| a == "--background"))?;
                return Ok(());
            }
            anyhow::bail!("Profile is already in use; choose another --profile directory: {error}");
        }
        Some(lock)
    } else {
        None
    };
    if !private {
        profile::apply_restore(&profile)?;
    }
    let (mut saved, recovery) = profile::load(&profile, private)?;
    if private {
        saved.settings.permissions.clear();
    }
    let event_loop = EventLoopBuilder::<Action>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_visible(!args.iter().any(|a| a == "--background"))
        .with_title(if private {
            "NoRAM — Private"
        } else {
            "NoRAM Browser"
        })
        .with_inner_size(tao::dpi::LogicalSize::new(1280, 850))
        .with_min_inner_size(tao::dpi::LogicalSize::new(760, 500))
        .build(&event_loop)?;
    let proxy = event_loop.create_proxy();
    if !private {
        instance::listen(&profile, proxy.clone())?;
    }
    let ipc = proxy.clone();
    // The trusted shell alone has IPC. Websites never receive a native command bridge.
    let mut context = WebContext::new(Some(profile.join("Interface")));
    let shell = WebViewBuilder::new_with_web_context(&mut context)
        // Wry's default args disable SmartScreen. Preserve the engine's protection.
        .with_additional_browser_args(runtime_args(debug_port))
        .with_incognito(private)
        .with_browser_extensions_enabled(false)
        .with_custom_protocol("noram-ui".into(), |_, request| {
            let (content, kind): (&[u8], &str) = match request.uri().path() {
                "/app.js" => (include_bytes!("../assets/app.js"), "text/javascript"),
                "/style.css" => (include_bytes!("../assets/style.css"), "text/css"),
                "/icons.svg" => (include_bytes!("../assets/icons.svg"), "image/svg+xml"),
                "/favicon.svg" => (include_bytes!("../assets/favicon.svg"), "image/svg+xml"),
                "/figtree.ttf" => (include_bytes!("../assets/vendor/figtree.ttf"), "font/ttf"),
                "/" => (
                    include_bytes!("../assets/shell.html"),
                    "text/html; charset=utf-8",
                ),
                _ => {
                    return wry::http::Response::builder()
                        .status(404)
                        .body(std::borrow::Cow::Borrowed(b"Not found".as_slice()))
                        .unwrap()
                }
            };
            wry::http::Response::builder()
                .header("Content-Type", kind)
                .body(std::borrow::Cow::Borrowed(content))
                .expect("Static shell response")
        })
        .with_url("noram-ui://localhost/")
        .with_navigation_handler(|url| url == "http://noram-ui.localhost/")
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |req| {
            if req.uri().host() != Some("noram-ui.localhost") {
                return;
            }
            if let Ok(value) = serde_json::from_str(req.body()) {
                let _ = ipc.send_event(Action::Command(value));
            }
        })
        .build(&window)?;
    platform::shortcuts(&shell, proxy.clone())?;
    drop(context);
    let (writer, writer_thread) = if private {
        (None, None)
    } else {
        let (tx, thread) = services::profile_writer(profile.clone(), proxy.clone());
        (Some(tx), Some(thread))
    };
    let settings = Rc::new(RefCell::new(saved.settings.clone()));
    let mut browser = Browser {
        window,
        shell: Some(shell),
        _profile_lock: profile_lock,
        tabs: Vec::new(),
        active: 0,
        next_id: 1,
        proxy,
        settings,
        blocker: Rc::new(RefCell::new(Blocker::load(&profile, private))),
        saved,
        content_context: WebContext::new(Some(profile.join("WebView"))),
        content_environment: None,
        extension_host: None,
        debug_port,
        profile: profile.clone(),
        private,
        panel: false,
        message: recovery.unwrap_or_default(),
        last_render: RefCell::new(String::new()),
        closed: Vec::new(),
        extensions: Vec::new(),
        vpn: json!({"profiles":[],"loaded":false}),
        writer,
        writer_thread,
        import_sources: Vec::new(),
        import_state: json!({"status":"idle"}),
        import_ticket: 0,
        pending_import: None,
        permissions: Default::default(),
        update_status: json!({"status":"idle"}),
        filter_status: updates::filter_status(&profile),
        filter_updating: false,
        app_updating: false,
    };
    let startup_url = args
        .iter()
        .find(|a| a.starts_with("https://") || a.starts_with("http://"))
        .cloned();
    // Restored pages are lazy: no network requests until explicitly selected.
    if !private && startup_url.is_none() {
        for entry in browser
            .saved
            .session
            .clone()
            .into_iter()
            .filter(|e| e.url != HOME)
            .take(100)
        {
            let id = browser.next_id;
            browser.next_id += 1;
            browser.tabs.push(Tab {
                id,
                title: entry.title,
                url: entry.url,
                view: None,
                pinned: entry.pinned,
                touched: Instant::now(),
                blocked: Rc::new(Cell::new(0)),
                loading: false,
                muted: entry.muted,
                activity: Default::default(),
                scroll: None,
                crashed: false,
                protection: String::new(),
            });
        }
    }
    browser.add(startup_url.unwrap_or_else(|| HOME.into()))?;
    if !private && browser.settings.borrow().auto_filter_updates && updates::filters_due(&profile) {
        browser.start_filter_update();
    }
    if !private && browser.settings.borrow().auto_app_updates {
        browser.start_app_update(false);
    }

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let result = match event {
            Event::UserEvent(Action::Quit)
            | Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                browser.permissions.borrow_mut().clear();
                browser.persist();
                browser.writer.take();
                if let Some(thread) = browser.writer_thread.take() {
                    let _ = thread.join();
                }
                for tab in &mut browser.tabs {
                    tab.view.take();
                }
                browser.extension_host.take();
                browser.content_environment.take();
                browser.content_context = WebContext::new(None);
                browser.shell.take();
                *flow = ControlFlow::Exit;
                Ok(())
            }
            Event::UserEvent(action) => browser.event(action),
            Event::WindowEvent {
                event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                ..
            } => {
                for t in &browser.tabs {
                    if let Some(v) = &t.view {
                        let _ = v.set_bounds(browser.bounds());
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            browser.message = e.to_string();
            eprintln!("{e:#}");
            browser.render();
        }
    });
}
