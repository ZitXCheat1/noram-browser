//! WebView2 policy is enforced natively, before scripts execute.
use crate::{
    policy::{host_matches, Blocker, Settings},
    Action,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tao::event_loop::EventLoopProxy;
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    core::{w, Interface, PWSTR},
    Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT},
};
use wry::{WebView, WebViewExtWindows};

fn core(view: &WebView) -> windows::core::Result<ICoreWebView2> {
    unsafe { view.controller().CoreWebView2() }
}
pub struct TabContext {
    pub activity: crate::lifecycle::SharedActivity,
    pub permissions: crate::permissions::Queue,
}
pub fn configure(
    view: &WebView,
    settings: Rc<RefCell<Settings>>,
    blocker: Rc<RefCell<Blocker>>,
    blocked: Rc<Cell<u64>>,
    proxy: EventLoopProxy<Action>,
    tab_id: u64,
    context: TabContext,
) -> anyhow::Result<()> {
    let TabContext {
        activity,
        permissions,
    } = context;
    unsafe {
        let web = core(view)?;
        let env = view.environment();
        let config = web.Settings()?;
        config.SetIsStatusBarEnabled(true)?;
        if let Ok(s) = config.cast::<ICoreWebView2Settings4>() {
            s.SetIsPasswordAutosaveEnabled(false)?;
            s.SetIsGeneralAutofillEnabled(false)?;
        }
        let profile = web.cast::<ICoreWebView2_13>()?.Profile()?;
        profile
            .cast::<ICoreWebView2Profile3>()?
            .SetPreferredTrackingPreventionLevel(COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT)?;
        let mut token = 0;
        let sources = proxy.clone();
        web.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(sender) = sender {
                    let mut uri = PWSTR::null();
                    sender.Source(&mut uri)?;
                    let uri = take_pwstr(uri);
                    if uri.starts_with("http") {
                        let _ = sources.send_event(Action::Source(tab_id, uri));
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        let popup_proxy = proxy.clone();
        web.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetHandled(true)?;
                let mut initiated = windows::core::BOOL::default();
                args.IsUserInitiated(&mut initiated)?;
                if initiated.as_bool() {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    let _ = popup_proxy.send_event(Action::New(take_pwstr(uri)));
                }
                Ok(())
            })),
            &mut token,
        )?;
        let permission_proxy = proxy.clone();
        let permission_settings = settings.clone();
        let permission_activity = activity.clone();
        web.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                // Keep the host in control of revocation and private-mode persistence.
                let Ok(extended) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() else {
                    return Ok(());
                };
                extended.SetSavesInProfile(false)?;
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let Some(origin) = crate::permissions::origin(&take_pwstr(uri)) else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let Some(kind) = crate::permissions::name(kind) else {
                    return Ok(());
                };
                if let Some(allow) = permission_settings
                    .borrow()
                    .permissions
                    .get(&origin)
                    .and_then(|m| m.get(kind))
                    .copied()
                {
                    if allow {
                        args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW)?;
                        if matches!(kind, "camera" | "microphone") {
                            permission_activity.capture.set(true);
                        }
                    }
                    return Ok(());
                }
                let mut queue = permissions.borrow_mut();
                if queue.len() >= 8 {
                    return Ok(());
                }
                static NEXT_PERMISSION: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(1);
                let id = NEXT_PERMISSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let deferral = args.GetDeferral()?;
                permission_activity
                    .permissions
                    .set(permission_activity.permissions.get() + 1);
                queue.push(crate::permissions::Request {
                    id,
                    tab: tab_id,
                    origin,
                    kind: kind.into(),
                    generation: permission_activity.generation.get(),
                    activity: permission_activity.clone(),
                    args: args.clone(),
                    deferral,
                });
                let _ = permission_proxy.send_event(Action::PermissionPrompt);
                Ok(())
            })),
            &mut token,
        )?;
        let audio_activity = activity.clone();
        let audio_proxy = proxy.clone();
        web.cast::<ICoreWebView2_8>()?
            .add_IsDocumentPlayingAudioChanged(
                &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                    if let Some(sender) = sender {
                        let mut playing = windows::core::BOOL::default();
                        sender
                            .cast::<ICoreWebView2_8>()?
                            .IsDocumentPlayingAudio(&mut playing)?;
                        audio_activity.audio.set(playing.as_bool());
                        let _ = audio_proxy.send_event(Action::Counters);
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        let download_activity = activity;
        let download_proxy = proxy.clone();
        web.cast::<ICoreWebView2_4>()?.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let operation = args.DownloadOperation()?;
                download_activity
                    .downloads
                    .set(download_activity.downloads.get() + 1);
                let activity = download_activity.clone();
                let proxy = download_proxy.clone();
                let finished = Rc::new(Cell::new(false));
                operation.add_StateChanged(
                    &StateChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(sender) = sender {
                            let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
                            sender.State(&mut state)?;
                            if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED
                                && !finished.replace(true)
                            {
                                activity
                                    .downloads
                                    .set(activity.downloads.get().saturating_sub(1));
                                let _ = proxy.send_event(Action::Counters);
                            }
                        }
                        Ok(())
                    })),
                    &mut 0,
                )?;
                let _ = download_proxy.send_event(Action::Counters);
                Ok(())
            })),
            &mut token,
        )?;
        let crash_proxy = proxy.clone();
        web.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                    {
                        let _ = crash_proxy.send_event(Action::Crashed(
                            tab_id,
                            kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
                        ));
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        if let Ok(new) = web.cast::<ICoreWebView2_22>() {
            new.AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        } else {
            web.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
        }
        let counters = proxy.clone();
        let last_count = Rc::new(Cell::new(std::time::Instant::now()));
        web.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                let mut method = PWSTR::null();
                request.Method(&mut method)?;
                let method = take_pwstr(method);
                let policy = settings.borrow();
                let mut source = PWSTR::null();
                let source = if let Some(sender) = sender {
                    sender.Source(&mut source)?;
                    take_pwstr(source)
                } else {
                    String::new()
                };
                let source_host = url::Url::parse(&source)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned));
                let exception = source_host.is_some_and(|host| {
                    policy
                        .exceptions
                        .iter()
                        .any(|rule| host_matches(&host, rule))
                });
                let insecure = policy.https_only
                    && url::Url::parse(&uri)
                        .is_ok_and(|u| u.scheme() == "http" && !crate::policy::local_host(&u));
                let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                args.ResourceContext(&mut context)?;
                let kind = match context {
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST
                    | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "xmlhttprequest",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => {
                        if source == uri {
                            "document"
                        } else {
                            "subdocument"
                        }
                    }
                    _ => "other",
                };
                if insecure
                    || (policy.shields
                        && !exception
                        && blocker
                            .borrow()
                            .network_request(&uri, &source, kind, &method))
                {
                    let response = env.CreateWebResourceResponse(
                        None,
                        403,
                        w!("Blocked by NoRAM"),
                        w!("Content-Type: text/plain\r\nCache-Control: no-store"),
                    )?;
                    args.SetResponse(&response)?;
                    blocked.set(blocked.get() + 1);
                    if blocked.get() == 1
                        || last_count.get().elapsed() > std::time::Duration::from_millis(80)
                    {
                        last_count.set(std::time::Instant::now());
                        let _ = counters.send_event(Action::Counters);
                    }
                } else if let Ok(headers) = request.Headers() {
                    headers.SetHeader(w!("DNT"), w!("1"))?;
                    headers.SetHeader(w!("Sec-GPC"), w!("1"))?;
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    shortcuts(view, proxy)?;
    Ok(())
}
pub fn shortcuts(view: &WebView, proxy: EventLoopProxy<Action>) -> anyhow::Result<()> {
    unsafe {
        let mut token = 0;
        view.controller().add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut key = 0;
                args.VirtualKey(&mut key)?;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let cmd = match (ctrl, shift, key) {
                    (true, _, 76) => "focus",
                    (true, false, 84) => "new",
                    (true, true, 84) => "reopen",
                    (true, _, 87) => "close",
                    (true, false, 9) => "next",
                    (true, true, 9) => "previous",
                    (true, true, 65) => "show_tabs",
                    (true, _, 72) => "show_history",
                    (true, true, 79) => "show_bookmarks",
                    (true, _, 70) => "show_find",
                    (true, _, 68) => "bookmark",
                    (true, _, 82) | (false, _, 116) => "reload",
                    (false, _, 27) => "stop",
                    (true, _, 80) => "print",
                    (true, true, 78) => "private",
                    (false, _, 123) => "devtools",
                    _ => return Ok(()),
                };
                args.SetHandled(true)?;
                let _ = proxy.send_event(Action::Command(serde_json::json!({"cmd":cmd})));
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}
pub fn back(v: &WebView) -> anyhow::Result<()> {
    unsafe {
        core(v)?.GoBack()?;
    }
    Ok(())
}
pub fn forward(v: &WebView) -> anyhow::Result<()> {
    unsafe {
        core(v)?.GoForward()?;
    }
    Ok(())
}
pub fn stop(v: &WebView) -> anyhow::Result<()> {
    unsafe {
        core(v)?.Stop()?;
    }
    Ok(())
}
pub fn clear_data(v: &WebView, proxy: EventLoopProxy<Action>) -> anyhow::Result<()> {
    unsafe {
        core(v)?
            .cast::<ICoreWebView2_13>()?
            .Profile()?
            .cast::<ICoreWebView2Profile2>()?
            .ClearBrowsingDataAll(&ClearBrowsingDataCompletedHandler::create(Box::new(
                move |result| {
                    result?;
                    let _ = proxy.send_event(Action::Refresh);
                    Ok(())
                },
            )))?;
    }
    Ok(())
}
pub fn mute(view: &WebView, muted: bool) -> anyhow::Result<()> {
    unsafe {
        core(view)?.cast::<ICoreWebView2_8>()?.SetIsMuted(muted)?;
    }
    Ok(())
}
pub fn downloads(view: &WebView) -> anyhow::Result<()> {
    unsafe {
        core(view)?
            .cast::<ICoreWebView2_9>()?
            .OpenDefaultDownloadDialog()?;
    }
    Ok(())
}
fn extension_profile(view: &WebView) -> anyhow::Result<ICoreWebView2Profile7> {
    unsafe {
        Ok(core(view)?
            .cast::<ICoreWebView2_13>()?
            .Profile()?
            .cast::<ICoreWebView2Profile7>()?)
    }
}
pub fn extensions(view: &WebView, proxy: EventLoopProxy<Action>) -> anyhow::Result<()> {
    unsafe {
        extension_profile(view)?.GetBrowserExtensions(&ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move|result,list|{
        if let Err(e)=result{let _=proxy.send_event(Action::Message(format!("Extensions unavailable: {e}")));return Ok(())}
        let mut entries=Vec::new();
        if let Some(list)=list{let mut count=0;list.Count(&mut count)?;for i in 0..count{let extension=list.GetValueAtIndex(i)?;let(mut id,mut name)=(PWSTR::null(),PWSTR::null());extension.Id(&mut id)?;extension.Name(&mut name)?;let mut enabled=windows::core::BOOL::default();extension.IsEnabled(&mut enabled)?;entries.push(serde_json::json!({"id":take_pwstr(id),"name":take_pwstr(name),"enabled":enabled.as_bool()}));}}
        let _=proxy.send_event(Action::Extensions(entries));Ok(())
    })))?;
    }
    Ok(())
}
pub fn add_extension(
    view: &WebView,
    path: &std::path::Path,
    proxy: EventLoopProxy<Action>,
) -> anyhow::Result<()> {
    let path = path.canonicalize()?;
    anyhow::ensure!(
        path.join("manifest.json").is_file(),
        "Select an unpacked extension folder containing manifest.json"
    );
    unsafe {
        extension_profile(view)?.AddBrowserExtension(&windows::core::HSTRING::from(path.to_string_lossy().as_ref()),&ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move|result,_|{
        let message=match result{Ok(())=>"Extension loaded. Reload existing website tabs to apply its content scripts.".into(),Err(e)=>format!("Could not load extension: {e}")};let _=proxy.send_event(Action::Message(message));let _=proxy.send_event(Action::Command(serde_json::json!({"cmd":"extensions"})));Ok(())
    })))?;
    }
    Ok(())
}
pub fn change_extension(
    view: &WebView,
    id: &str,
    enabled: Option<bool>,
    proxy: EventLoopProxy<Action>,
) -> anyhow::Result<()> {
    let id = id.to_owned();
    unsafe {
        extension_profile(view)?.GetBrowserExtensions(
            &ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |result, list| {
                result?;
                if let Some(list) = list {
                    let mut count = 0;
                    list.Count(&mut count)?;
                    for i in 0..count {
                        let extension = list.GetValueAtIndex(i)?;
                        let mut found = PWSTR::null();
                        extension.Id(&mut found)?;
                        if take_pwstr(found) == id {
                            let proxy = proxy.clone();
                            if let Some(enabled) = enabled {
                                extension.Enable(
                                    enabled,
                                    &BrowserExtensionEnableCompletedHandler::create(Box::new(
                                        move |result| {
                                            let message = match result {
                                                Ok(()) => "Extension updated.".into(),
                                                Err(e) => e.to_string(),
                                            };
                                            let _ = proxy.send_event(Action::Message(message));
                                            let _ = proxy.send_event(Action::Command(
                                                serde_json::json!({"cmd":"extensions"}),
                                            ));
                                            Ok(())
                                        },
                                    )),
                                )?;
                            } else {
                                extension.Remove(
                                    &BrowserExtensionRemoveCompletedHandler::create(Box::new(
                                        move |result| {
                                            let message = match result {
                                                Ok(()) => "Extension removed.".into(),
                                                Err(e) => e.to_string(),
                                            };
                                            let _ = proxy.send_event(Action::Message(message));
                                            let _ = proxy.send_event(Action::Command(
                                                serde_json::json!({"cmd":"extensions"}),
                                            ));
                                            Ok(())
                                        },
                                    )),
                                )?;
                            }
                            return Ok(());
                        }
                    }
                }
                let _ = proxy.send_event(Action::Message(
                    "Extension was not found. Refresh the list.".into(),
                ));
                Ok(())
            })),
        )?;
    }
    Ok(())
}

pub fn import_cookies(
    view: &WebView,
    cookies: &[crate::importer::Cookie],
) -> anyhow::Result<(usize, usize)> {
    unsafe {
        let manager = core(view)?.cast::<ICoreWebView2_2>()?.CookieManager()?;
        let mut imported = 0;
        let mut failed = 0;
        for c in cookies {
            let result = (|| -> windows::core::Result<()> {
                let cookie = manager.CreateCookie(
                    &windows::core::HSTRING::from(&c.name),
                    &windows::core::HSTRING::from(&c.value),
                    &windows::core::HSTRING::from(&c.domain),
                    &windows::core::HSTRING::from(&c.path),
                )?;
                cookie.SetIsSecure(c.secure)?;
                cookie.SetIsHttpOnly(c.http_only)?;
                cookie.SetSameSite(COREWEBVIEW2_COOKIE_SAME_SITE_KIND(c.same_site))?;
                if let Some(expires) = c.expires {
                    cookie.SetExpires(expires)?;
                }
                manager.AddOrUpdateCookie(&cookie)
            })();
            if result.is_ok() {
                imported += 1
            } else {
                failed += 1
            }
        }
        Ok((imported, failed))
    }
}

pub fn probe(
    view: &WebView,
    id: u64,
    generation: u64,
    proxy: EventLoopProxy<Action>,
) -> anyhow::Result<()> {
    unsafe {
        core(view)?.ExecuteScript(
            &windows::core::HSTRING::from(crate::lifecycle::PROBE),
            &ExecuteScriptCompletedHandler::create(Box::new(move |result, raw| {
                let raw = if result.is_ok() { raw } else { String::new() };
                let _ = proxy.send_event(Action::SleepProbe(id, generation, raw));
                Ok(())
            })),
        )?;
    }
    Ok(())
}

pub fn save_pdf(
    view: &WebView,
    path: &std::path::Path,
    proxy: EventLoopProxy<Action>,
) -> anyhow::Result<()> {
    let path = std::path::absolute(path)?;
    unsafe {
        core(view)?.cast::<ICoreWebView2_7>()?.PrintToPdf(
            &windows::core::HSTRING::from(path.to_string_lossy().as_ref()),
            None,
            &PrintToPdfCompletedHandler::create(Box::new(move |result, success| {
                let message = if result.is_ok() && success {
                    "PDF saved.".into()
                } else {
                    "Could not save PDF.".into()
                };
                let _ = proxy.send_event(Action::Message(message));
                Ok(())
            })),
        )?;
    }
    Ok(())
}
pub fn download_directory(view: &WebView, path: &std::path::Path) -> anyhow::Result<()> {
    let path = path.canonicalize()?;
    unsafe {
        core(view)?
            .cast::<ICoreWebView2_13>()?
            .Profile()?
            .SetDefaultDownloadFolderPath(&windows::core::HSTRING::from(
                path.to_string_lossy().as_ref(),
            ))?;
    }
    Ok(())
}
