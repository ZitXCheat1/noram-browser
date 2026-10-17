# Verification — 17 October 2026

Tested locally on Windows with Rust 1.97.0 and the installed Evergreen WebView2 Runtime. The app is Windows-only. Tests used disposable `--profile` directories and loopback fixtures, separate from the visible interactive browser.

![Actual NoRAM desktop webview](start-page.png)

## Build and policy checks

- `cargo fmt --check` passed.
- `cargo test --locked --offline` passed: four tests covering domain boundaries/subdomains, parameter preservation, HTTPS upgrade, local development, search encoding, and rejection of unsafe schemes/embedded credentials.
- `cargo clippy --locked --offline --all-targets -- -D warnings` passed. Windows sandbox emitted an infrastructure warning about copying incremental files when hard links were unavailable; there were no code diagnostics.
- `cargo build --release --locked --offline` passed. One Cargo job is configured to avoid the memory spikes observed with parallel compilation of generated Windows bindings.

## Native app checks

`scripts/smoke.js` ran through Playwright attached to the actual optimized desktop app's WebView2 targets. This was not a website mock of a browser. Fourteen grouped checks passed:

1. A real local page rendered in its own content webview.
2. Navigating with `utm_source=smoke&q=kept` retained `q=kept` and removed the tracking parameter.
3. A real HTTP fixture received `DNT: 1` and `Sec-GPC: 1`.
4. A geolocation request returned permission-denied through native policy.
5. A request to the reserved blocked test domain returned HTTP 403 from the native interceptor and incremented the counter.
6. Bookmark creation appeared in the saved state.
7. Native back and forward loaded the expected fixture URLs.
8. A scripted popup after transient user activation expired did not create a tab.
9. A user-clicked `target="_blank"` link created a tab.
10. A one-page live budget unloaded the least recently used background webview.
11. Selecting an unloaded tab reloaded it while staying within that budget.
12. Pinning a tab exempted it from unloading, allowing two live pages with a budget of one.
13. `javascript:` navigation was rejected.
14. Public HTTP upgraded to HTTPS, and the trusted UI produced no JavaScript exceptions throughout the suite. These are separate assertions grouped in the result output.

The reserved `ads.noram.test` rule verifies the application interceptor without a dependency on an advertising company's network. A separate exploratory request to `doubleclick.net` was blocked earlier by the engine's strict tracking prevention, so it did not prove the application's own 403 path. Requests blocked by engine tracking prevention are not included in the application counter.

`scripts/private-smoke.js` verified that the private shell reports private mode, private content cannot read normal localStorage, normal content cannot read private localStorage, and a private bookmark works in memory. A SHA-256 comparison confirmed `browser.json` was unchanged during the private test. This did not test forensic erasure, every storage API, or all network-based identification.

`scripts/restore-smoke.js` verified that a restart restored three website tabs with zero live website webviews, retained the bookmark and normal site storage, resumed a selected tab, and cleared localStorage through native `ClearBrowsingDataAll` with its success callback. A second normal process targeting the same test profile exited with the explicit profile-lock error.

## Reproduce the smoke suite

Use fresh test profiles; the smoke script changes settings and bookmarks and expects a clean starting state. Build the release binary first. Start the loopback fixture in one terminal:

```powershell
node scripts/qa-server.cjs
```

In another terminal:

```powershell
New-Item -ItemType Directory -Force output/playwright | Out-Null
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9231 --remote-debugging-address=127.0.0.1'
.\target\release\noram-browser.exe --profile artifacts/smoke-profile --background
```

Attach the installed Playwright CLI from another terminal:

```powershell
playwright-cli -s=noram-smoke attach --cdp http://127.0.0.1:9231
playwright-cli -s=noram-smoke snapshot
playwright-cli -s=noram-smoke run-code --filename scripts/smoke.js
```

For the private check, launch a second instance with the same environment, `--private --profile artifacts/smoke-profile --background`, then run `scripts/private-smoke.js` in the existing attached session. Compare the SHA-256 of `browser.json` before/after private actions. Close the private test process; restart the normal test instance with the same profile and run `scripts/restore-smoke.js`.

Only enable remote debugging for disposable test instances. Remove `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` in that terminal afterward, and close the test instances and fixture. Normal launches do not request a debugging port. Scripts invoke the same Rust commands as the UI; the main navigation and website links also use real UI interactions.

## Not established

No comparative performance benchmark was performed. This does not establish lower RAM, CPU, startup latency or page-load latency than Chrome, Brave or Firefox. Working-set totals need the entire WebView2 process tree and careful handling of shared memory. A live-view count is an architectural check, not a measurement of memory saved.

Download dialogs, printing, webcam/microphone flows, accessibility with screen readers, hostile-site penetration testing, DRM, extensions, long-running stability, all shortcut combinations, Windows installation packaging, and clean-machine setup were not comprehensively tested. CodeRabbit was unavailable; review was local, supported by Rust checks and the native smoke suite. GitHub Actions is configured independently; local success is not a claim that hosted CI has completed.
