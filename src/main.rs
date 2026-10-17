#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod platform;
mod policy;

use anyhow::{Context, Result};
use policy::{address, clean_url, Blocker, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
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
const BAR: f64 = 110.;

#[derive(Debug, Clone)]
enum Action {
    Command(Value),
    Title(u64, String),
    Location(u64, String),
    Loaded(u64),
    New(String),
    Refresh,
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    title: String,
    url: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    settings: Settings,
    bookmarks: Vec<Entry>,
    history: Vec<Entry>,
    session: Vec<Entry>,
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
}

struct Browser {
    window: Window,
    shell: WebView,
    _profile_lock: Option<std::fs::File>,
    tabs: Vec<Tab>,
    active: u64,
    next_id: u64,
    proxy: EventLoopProxy<Action>,
    settings: Rc<RefCell<Settings>>,
    blocker: Rc<Blocker>,
    saved: Saved,
    profile: PathBuf,
    private: bool,
    panel: bool,
    message: String,
    last_sync: Instant,
}

impl Browser {
    fn bounds(&self) -> Rect {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        Rect {
            position: LogicalPosition::new(0., BAR).into(),
            size: LogicalSize::new(size.width.max(1.), (size.height - BAR).max(1.)).into(),
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
            .map(|t| Entry {
                title: t.title.clone(),
                url: t.url.clone(),
            })
            .collect();
        let file = self.profile.join("browser.json");
        let tmp = self.profile.join("browser.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.saved)?)?;
        std::fs::rename(tmp, file)?;
        Ok(())
    }
    fn persist(&mut self) {
        if let Err(e) = self.save() {
            self.message = format!("Could not save profile: {e}");
        }
    }
    fn render(&self) {
        let data = json!({
            "tabs": self.tabs.iter().map(|t| json!({"id":t.id,"title":t.title,"url":t.url,"sleeping": t.view.is_none() && t.url != HOME,"pinned":t.pinned,"loading":t.loading})).collect::<Vec<_>>(),
            "active":self.active,"private":self.private,"settings":*self.settings.borrow(),
            "bookmarks":self.saved.bookmarks,"history":self.saved.history.iter().rev().take(150).collect::<Vec<_>>(),
            "blocked":self.tabs.iter().map(|t| t.blocked.get()).sum::<u64>(),
            "live":self.tabs.iter().filter(|t|t.view.is_some()).count(),"rules":self.blocker.len(),"message":self.message
        });
        let _ = self
            .shell
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
        while self.tabs.iter().filter(|t| t.view.is_some()).count() > budget {
            let victim = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, t)| t.id != self.active && !t.pinned && t.view.is_some())
                .min_by_key(|(_, t)| t.touched)
                .map(|(i, _)| i);
            if let Some(i) = victim {
                self.tabs[i].view.take();
                self.tabs[i].loading = false;
            } else {
                break;
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
        let view = WebViewBuilder::new()
            .with_environment(self.shell.environment())
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
        )?;
        view.load_url(&self.tabs[i].url)?;
        self.tabs[i].view = Some(view);
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
            .position(|t| t.id == self.active)
            .unwrap_or(0);
        match cmd {
            "ready" => {}
            "navigate" => self.navigate(value["url"].as_str().unwrap_or(""))?,
            "new" => self.add(HOME.into())?,
            "switch" => self.activate(value["id"].as_u64().unwrap_or(0))?,
            "close" => {
                let id = value["id"].as_u64().unwrap_or(self.active);
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
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
            }
            "sleep" => {
                if !self.tabs[i].pinned {
                    self.tabs[i].view.take();
                    self.tabs[i].loading = false;
                    self.add(HOME.into())?;
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
            "settings" => {
                let previous = self.settings.borrow().clone();
                let mut settings: Settings = serde_json::from_value(value["settings"].clone())?;
                settings.max_live_tabs = settings.max_live_tabs.clamp(1, 16);
                settings.exceptions = previous.exceptions;
                *self.settings.borrow_mut() = settings;
                self.enforce_budget();
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
            "clear_data" => platform::clear_data(&self.shell, self.proxy.clone())?,
            "private" => {
                std::process::Command::new(std::env::current_exe()?)
                    .arg("--private")
                    .arg("--profile")
                    .arg(&self.profile)
                    .spawn()?;
            }
            "focus" => {
                self.shell.focus()?;
                self.shell.evaluate_script("focusAddress()")?;
            }
            "next" => {
                let next = (i + 1) % self.tabs.len();
                self.activate(self.tabs[next].id)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn event(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Command(v) => self.command(v)?,
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
                    t.loading = true;
                }
            }
            Action::Loaded(id) => {
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
        lock.try_lock().map_err(|e| {
            anyhow::anyhow!("Profile is already in use; choose another --profile directory: {e}")
        })?;
        Some(lock)
    } else {
        None
    };
    let saved: Saved = match std::fs::read(profile.join("browser.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .context("Invalid browser.json; preserve it and repair or choose --profile")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Saved::default(),
        Err(e) => return Err(e.into()),
    };
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
    let ipc = proxy.clone();
    // The trusted shell alone has IPC. Websites never receive a native command bridge.
    let mut context = WebContext::new(Some(profile.join("WebView")));
    let shell = WebViewBuilder::new_with_web_context(&mut context)
        // Wry's default args disable SmartScreen. Preserve the engine's protection.
        .with_additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI")
        .with_incognito(private)
        .with_custom_protocol("noram-ui".into(), |_, _| {
            wry::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .body(std::borrow::Cow::Borrowed(
                    include_bytes!("../assets/shell.html").as_slice(),
                ))
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
    let settings = Rc::new(RefCell::new(saved.settings.clone()));
    let mut browser = Browser {
        window,
        shell,
        _profile_lock: profile_lock,
        tabs: Vec::new(),
        active: 0,
        next_id: 1,
        proxy,
        settings,
        blocker: Rc::new(Blocker::default()),
        saved,
        profile,
        private,
        panel: false,
        message: String::new(),
        last_sync: Instant::now(),
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
                pinned: false,
                touched: Instant::now(),
                blocked: Rc::new(Cell::new(0)),
                loading: false,
            });
        }
    }
    browser.add(startup_url.unwrap_or_else(|| HOME.into()))?;
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(2));
        let result = match event {
            Event::UserEvent(action) => browser.event(action),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                browser.persist();
                *flow = ControlFlow::Exit;
                Ok(())
            }
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
            Event::MainEventsCleared if browser.last_sync.elapsed() > Duration::from_secs(2) => {
                browser.last_sync = Instant::now();
                for t in &mut browser.tabs {
                    if let Some(v) = &t.view {
                        if let Ok(url) = v.url() {
                            if url.starts_with("http") {
                                t.url = url;
                            }
                        }
                    }
                }
                browser.render();
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
