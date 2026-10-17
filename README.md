# NoRAM Browser

A Windows browser built with a Rust application shell and Microsoft Edge WebView2. It is an experimental browser, not a new rendering engine. “NoRAM” is a name, not a zero-memory claim.

The goal is to keep fewer pages alive, block known advertising and analytics requests, and offer a small, understandable browser. There is **no measured speed, RAM, or CPU advantage over Chrome, Brave, or Firefox yet**, and this release does not have their complete feature set.

![NoRAM start page](docs/start-page.png)

## Run

Requirements: Windows 10/11, a current [Evergreen WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/), Rust stable (1.89+), and Visual Studio C++ Build Tools with a Windows SDK. Engine security updates are delivered by Microsoft's runtime updater; the Rust app itself has no automatic updater.

```powershell
cargo run --release --locked
```

Or build and run the executable:

```powershell
cargo build --release --locked
.\target\release\noram-browser.exe
.\target\release\noram-browser.exe --private
.\target\release\noram-browser.exe --profile C:\path\to\separate-profile
```

An optional HTTP/HTTPS URL argument opens a page on launch. `--background` hides the native window for automation; it does not turn the app into a background service.

## Included

- Real Chromium website rendering, multiple tabs, close/switch, back/forward, reload/stop, search and address bar.
- Native request blocking using an auditable, bundled starter list (60 advertising/analytics domains plus a reserved `.test` domain for verification). Domain suffixes match subdomains without substring overmatching. No remote list downloads, subscription service, or extension required.
- Strict WebView2 tracking prevention; optional HTTP-to-HTTPS upgrade with insecure public resources blocked; tracking-parameter removal on navigations; DNT and Sec-GPC request signals.
- All website permissions denied by default: camera, microphone, geolocation, notifications and clipboard-read requests. No permission-approval UI in this version.
- Site exceptions for the domain blocker; the engine's strict tracking prevention remains active.
- Private windows using WebView2 InPrivate controllers. No NoRAM session, history, preferences, or bookmark changes are written from private windows.
- Bookmarks, bounded local history, lazy session restore, and atomic JSON profile updates.
- Live-tab budget (default four content webviews), least-recently-used background tab unloading, explicit unload, and pin/keep-awake controls. Pinned tabs may exceed the budget.
- Find text, zoom, print / save PDF, developer tools, engine context menus, engine download dialogs, and clear cookies/cache/site storage.
- A local start page and controls with no external fonts, scripts, telemetry, account system, or backend.

## Important limits

Unloading destroys the page webview. Selecting it reloads its last URL, **losing unsaved forms, playback, scroll and in-tab back history**. Pin anything important before opening more tabs. Background audio, active calls, forms and downloads are not automatically detected. Pins and the selected tab are not restored across launches. Session restore keeps URLs and titles, loads pages only on selection, and adds a fresh start tab.

Blocking is a small domain-denylist system, not uBlock Origin or Brave Shields parity. It has no EasyList parser, cosmetic filtering, scriptlets, automatic list updates, per-request log, or guaranteed YouTube ad removal. Worker coverage depends on the installed runtime; modern runtimes use request-source filtering. WebSocket and other engine-internal traffic are not comprehensively filtered. The counter covers intercepted blocked requests; it is not a privacy score or estimate of bytes/time saved.

Strict tracking prevention and HTTPS-only behavior can break sites. Disabling the ad blocker does not disable engine tracking prevention. Cleaning tracking parameters can break signed or specialized URLs. Localhost HTTP is allowed for development. Permission denial breaks calls, location features and similar workflows. DNT/GPC depend on site support; these signals do not make browsing anonymous.

Private browsing does not hide your IP, stop network observation, guarantee fingerprint resistance or erase downloaded files. Private windows may share an InPrivate session while open. Normal bookmarks and existing history are loaded from the selected profile, but history is hidden in the private UI and no private changes are persisted. Runtime caches and operating-system traces are outside the no-history guarantee.

Not included: extension-store compatibility, sync, password manager, tab groups, reader mode, translation, containers, VPN/Tor, built-in DoH configuration, OS default-browser registration, import/export UI, full download manager, permission grants, crash recovery for unsaved page state, browser auto-updater, macOS/Linux packaging, or full Chrome/Brave/Firefox feature parity. DRM and browser-specific login flows may not work. This is an early prototype, not an audited hardened browser.

## Shortcuts

| Shortcut | Action |
|---|---|
| Ctrl+L | Focus address bar |
| Ctrl+T / Ctrl+W | New / close tab |
| Ctrl+Tab | Next tab |
| Ctrl+D | Toggle bookmark |
| Ctrl+R / F5 | Reload |
| Escape | Stop page loading |
| Ctrl+P | Print / save PDF |
| Ctrl+Shift+N | New private window |
| F12 | Website developer tools |

## Data and architecture

Normal browsing data lives in `%LOCALAPPDATA%\NoRamBrowser`, or the `--profile` directory:

- `browser.json`: preferences, bookmarks, history (last 2,000 entries) and tab URLs/titles. The UI shows the most recent 150 history entries. JSON is plaintext; rely on your OS account permissions and disk encryption.
- `browser.lock`: exclusive lock preventing two normal instances from overwriting the same profile. Use another profile or a private window for additional instances.
- `WebView/`: engine profile managed by WebView2.

`src/main.rs` owns tabs and the event loop. `src/policy.rs` handles URL sanitation and domain matching. `src/platform.rs` installs native WebView2 request/permission handlers and accelerators. `assets/shell.html` is the trusted UI.

The UI is served from a fixed custom-protocol origin, has a restrictive content security policy, and rejects navigation away from that origin. Only this webview has a Rust command bridge. Website webviews have **no application IPC handler**. Website strings are rendered with DOM `textContent`; no website-supplied HTML is inserted into the shell. Website blocking/permissions are installed before the first untrusted URL loads. Tabs reuse a WebView2 environment; this keeps the engine's multi-process sandbox model. No `--no-sandbox` flags are used. Wry's default SmartScreen-disable argument is overridden to preserve the engine's protection.

Technical references: [Wry](https://github.com/tauri-apps/wry), [WebView2 documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/), [request interception](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/webresourcerequested), [tracking prevention](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/tracking-prevention).

## Development and verification

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
```

See [docs/verification.md](docs/verification.md) for real-app test results and a reproducible local debug workflow. Do not compare only the Rust process's working set to an entire competing browser: include the full WebView2 process tree. Shared pages, viewport, cache state, idle time, extensions and runtime versions must be controlled. Private working set, committed memory, CPU time, navigation latency and discarded-tab restore cost should be measured separately.

## License

MIT for this application. Dependencies and the installed WebView2 Runtime retain their respective licenses.
