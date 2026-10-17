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
pub fn configure(
    view: &WebView,
    settings: Rc<RefCell<Settings>>,
    blocker: Rc<Blocker>,
    blocked: Rc<Cell<u64>>,
    proxy: EventLoopProxy<Action>,
) -> anyhow::Result<()> {
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
        web.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
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
        web.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
                let Some(args) = args else { return Ok(()) };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
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
                if insecure || (policy.shields && !exception && blocker.blocks(&uri)) {
                    let response = env.CreateWebResourceResponse(
                        None,
                        403,
                        w!("Blocked by NoRAM"),
                        w!("Content-Type: text/plain\r\nCache-Control: no-store"),
                    )?;
                    args.SetResponse(&response)?;
                    blocked.set(blocked.get() + 1);
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
                    (true, _, 87) => "close",
                    (true, _, 9) => "next",
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
