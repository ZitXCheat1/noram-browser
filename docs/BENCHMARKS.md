# Optional resource measurements

Benchmarks are opt-in and are never started by the application, `npm test`, or CI.
Running `node scripts/benchmark.cjs` without `--run` starts no browsers.

To deliberately run a comparison on Windows:

```powershell
cargo build --release --locked
npm ci --ignore-scripts
node scripts/benchmark.cjs --run --tabs 10 --runs 3
```

The runner creates fresh temporary profiles. It detects installed Edge, Chrome and
Brave, skips unavailable browsers, and compares NoRAM with a budget of four and a
budget equal to the workload size. The fixture serves identical local pages with
a touched 4 MiB allocation per tab. Existing personal profiles are never used.

`output/benchmarks/latest.json` records browser/runtime versions, machine details,
process-tree working sets, private committed bytes, cumulative CPU time, startup
readiness, page loading, and tab activation including sleeping-tab recreation.
The cleanup helper targets only processes whose command line contains the exact
temporary root created for that run.

These measurements have important limits:

- NoRAM uses hidden WebView2 views; other browsers use headless Chromium. Rendering
  modes differ, so these results cannot establish an everyday browser advantage.
- Summed working sets double-count shared pages. Private bytes measure committed
  memory, not private resident RAM. The complete process tree includes NoRAM's
  separate trusted UI environment.
- Browser profiles are fresh, but Windows file caches and background load are
  uncontrolled. CPU deltas can be distorted when child processes exit.
- The synthetic workload does not represent video, conferencing, large documents,
  extensions, or typical mixed browsing. Automation adds timing overhead.

No completed comparative result is published with this change. A local trial was
stopped at the user's request; its partial output is not valid benchmark evidence.
