# Browser reliability changes — 23 October 2026

This branch adds conservative tab protection, origin-scoped permissions,
renderer-crash recovery, profile backups and recovery, bookmark export, filter
updates, Windows packaging, a signed-update path, keyboard tab navigation, reduced
motion and high-contrast support, and an automated native browser test runner.

## Checks performed locally

- Rust formatting, all 15 unit/fixture tests, and Clippy with warnings denied passed.
- A release build passed before the final shutdown-cleanup adjustment; final code
  is also compiled by the Rust tests and Clippy.
- Existing native browsing, session restoration, extension and import suites passed.
- The initial readiness suite passed editable-page and media protection, website
  command isolation, permission prompts, remembered denial and reset, scroll
  restoration, keyboard tab navigation, and visibility of backup/export controls.
- Private import restrictions and unchanged normal-profile JSON passed.

The later additions for native PDF export, active-download protection, renderer
crash/reload and single-instance forwarding are included in CI. Their final local
browser run was interrupted when the user requested that benchmark/test browser
processes be closed. No further local browser runs were started. Consult the
branch's GitHub Actions run for the final automated result.

## Remaining validation boundaries

- A real publisher certificate and release secrets are required before producing
  signed installers or enabling installation of app updates. See [release setup](RELEASE.md).
- Clean-machine installation, uninstall, signed upgrade, physical camera/microphone,
  interactive printer dialogs, screen-reader testing, and a long-duration browsing
  soak require dedicated validation. Keyboard support is not a full accessibility audit.
- Protection is intentionally conservative. Editable controls, frames and media
  can keep pages awake even when idle. Custom apps with unsaved state in JavaScript
  still need pinning. Capturing tabs stay protected until navigation; interrupted
  downloads stay protected until closed. Reloading cannot recover unsaved page state.
- No completed comparative benchmark is claimed. The optional runner is excluded
  from application startup, browser regression CI and `npm test`.

## Automated native checks

After building the release executable, `npm ci --ignore-scripts` and `npm test`
run the suites in temporary profiles against a local-only fixture server. The
runner writes screenshots and explicit pass/fail results to `output/playwright/`.
It releases native views on normal close and cleans up only processes associated
with the exact temporary test-profile root. It does not attach to personal browser
profiles. CI uploads the results even when a suite fails.
