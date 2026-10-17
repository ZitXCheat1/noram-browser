# Third-party assets and libraries

The Rust application source is MIT licensed. Bundled assets and dependencies retain their own licenses.

## IcoMoon Free

Icons by Keyamoon / IcoMoon, https://github.com/Keyamoon/IcoMoon-Free and https://icomoon.io/.
Used under CC BY 4.0: https://creativecommons.org/licenses/by/4.0/.
Original license is in `assets/vendor/icomoon-license.txt`; original source data is `assets/vendor/icomoon.json`.
`assets/icons.svg` extracts selected paths into an SVG symbol sprite, with stable aliases; the browser applies a small same-color round-join stroke to soften corners. Glyph geometry otherwise remains unchanged.

## Figtree

Figtree variable font by Erik Kennedy and contributors, distributed by Google Fonts:
https://github.com/google/fonts/tree/main/ofl/figtree.
SIL Open Font License 1.1, included in `assets/vendor/figtree-license.txt`.
The font file is bundled unmodified. Proxima Nova is not bundled; installed copies can be selected by the CSS font stack. No DuckDuckGo font files are copied or fetched at runtime.

## Filter lists

EasyList and EasyPrivacy by The EasyList authors, https://easylist.to/.
Unmodified snapshots fetched on 17 October 2026 from:

- https://easylist.to/easylist/easylist.txt
- https://easylist.to/easylist/easyprivacy.txt

Bundled under Creative Commons Attribution-ShareAlike 3.0 Unported (or a later version), https://creativecommons.org/licenses/by-sa/3.0/. See https://easylist.to/pages/licence.html. Source headers, version and commit identifiers remain in the bundled files. The lists remain independently licensed under those terms; they are not relicensed as MIT. A small separate domain starter list in `assets/blocklist.txt` adds local policies and reserved test-domain coverage.

## adblock-rust

Brave Software's adblock-rust 0.13.3: https://github.com/brave/adblock-rust.
MPL-2.0, https://www.mozilla.org/MPL/2.0/. Used as an unmodified crates.io dependency, recorded in Cargo.lock. Its MPL-covered source remains available from the upstream repository and crates.io. No modified copies of that library are included.

Other Rust dependency licenses are available in their crates.io packages. Microsoft WebView2 is installed separately and retains Microsoft's license and update policy.
