# P6 milestone 1 — destructive adjustments and histogram

Completed M1 on main on 2026-10-24 through implementation `7a845a1`. The user's
four choices and permission to start
P6 with P5's comparison still pending are recorded in DECISIONS.md. This report
covers M1 only. P6 is not complete; the P5/P6 completion tags remain pending.
Work stops before M2.

## Shipped

- Brightness/contrast, levels, master RGB curves with editable control points,
  exposure and hue/saturation, with explicit Apply and Reset controls.
- Browser-independent Rust kernels, prepared channel LUTs, bounded COW tile jobs,
  one named undo step per changed application, exact undo/redo and selection clipping.
- Exact alpha and fully transparent hidden RGB preservation; pixel/all/parent
  locks enforced. M1 targets raster pixels and visibly rejects mask/Quick Mask/group
  targets.
- An always-available panel with exact 256-bin RGB/luma histograms for the full
  active raster. Counts use stored alpha weights and exclude hidden RGB; layer
  masks/opacity/blend and selection are outside this named histogram source.
- Worker-owned jobs, next-tile hydration, approximately four-millisecond slices,
  cancellation before atomic commit, CPU residency accounting, stale-result
  protection and retries after transient background-histogram conflicts.
- Browser/desktop integration checks, project and crash-recovery sample checks,
  and a reproducible hardware benchmark. No new external dependency: core and
  already-inventoried MIT/Apache-2.0 serde supply the filters crate's dependencies.

The numerical definitions are documented in P6-DESIGN.md. They do not establish
Photoshop formula equivalence. Histograms operate on exact full-resolution data;
they are not the deferred GPU viewport-preview feature.

## Measured 24 MP apply and histogram

Windows 10 Pro 10.0.19045, Intel Xeon E5-2680 v3 at 2.50 GHz, about 32 GB RAM,
NVIDIA GeForce GTX 1650, driver 27.21.14.5751. Installed Chrome 156.0.8078.47 and
Edge 155.0.4283.62, each using WebGPU and WebGL2. The adapter probe reports
NVIDIA/Turing and `isFallbackAdapter=false`; software-rendering evidence is separate.
Headless viewport 1280×850, DPR 1. Hardware runs execute serially after the frozen
production build, without simultaneous local builds or other test suites.

The fixture is the same NASA/JPL-Caltech/IPGP photo documented in P5-REPORT.md,
cropped in Rust from 6002×4000 to 6000×4000 without resampling, 15,344,787 PNG
bytes, SHA-256 `603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`.
[Original photo](https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/).
All 24,000,000 opaque pixels are processed, without a selection. Exposure uses
+0.75 stops, zero offset, gamma 1. HSL uses +20° hue, +15% saturation, zero lightness.

Each scenario has one first application plus five warm trials. Undo restores the
original before each call. Source samples are already resident after opening;
these are not disk-cold paging measurements. One worker thread performs the CPU
work. Return timing includes bridge dispatch, yielding, candidate processing and
command commit. Visible timing additionally waits for the normal presentation
acknowledgement, including display regeneration/upload; it excludes physical
input, compositor/scanout and GPU timestamps. Histogram timing is a separate call.

| Browser / API   | Exposure first / warm p50 / p95 | HSL first / warm p50 / p95 | Histogram warm p50 / worst p95 |
| --------------- | ------------------------------: | -------------------------: | -----------------------------: |
| Chrome / WebGPU |         1.296 / 1.353 / 1.908 s |    2.295 / 2.276 / 2.367 s |          0.513–0.515 / 0.625 s |
| Chrome / WebGL2 |         1.178 / 1.398 / 1.887 s |    2.310 / 2.271 / 2.384 s |          0.497–0.500 / 0.607 s |
| Edge / WebGPU   |         1.381 / 1.392 / 1.999 s |    2.283 / 2.286 / 2.365 s |          0.501–0.527 / 0.613 s |
| Edge / WebGL2   |         1.186 / 1.346 / 1.986 s |    2.293 / 2.327 / 2.385 s |          0.502–0.507 / 0.569 s |

With five warm samples, nearest-rank p95/p99 both equal the slowest trial. Maximum
warm visible p95 was 2.186 s for Exposure and 2.560 s for HSL. Every histogram
reported exactly 24,000,000 nontransparent pixels and alpha weight 6,120,000,000.
The other three adjustments are functionally tested; their 24 MP timings are not
claimed by this representative LUT/HSL measurement.

Maximum sampled logical CPU residency was 289 MiB, GPU 16.11 MiB. Adding a further
1 MiB allowance to the conservative sum of CPU category maxima gives 290 MiB,
below 768 MiB. Scratch can peak between frame samples; admission checks remain
authoritative. These figures describe buffer capacities, excluding allocator
high-water pages, map/Arc headers, JavaScript objects and driver overhead. They
are not process RAM or VRAM measurements. Evidence, full trials, category maxima,
kernel/visible timings, hardware and WASM SHA-256 are in
`bench/results/p6-m1-adjustment-performance.json`; reproduce with `pnpm bench:adjustments`.

## Existing hardware regressions

The deterministic Rust-generated 6000×4000 RGBA8 fixture (67,038,649 PNG bytes)
uses the same viewport and four browser/API paths. First opens measured
1.11–1.38 s, the next two warm opens 0.92–1.09 s. Four-second pan, zoom and
cross-mip zoom trials measured 59.75–60.00 fps, worst period p95/p99
16.745/16.775 ms. Refinement after cross-mip zoom stopped took 30.09–45.84 ms
while the cached preview was already visible. Evidence:
`bench/results/p6-m1-viewport-performance.json`.

The 300-pixel soft round brush uses 10% spacing at 100%, 120 Hz native protocol
mouse input at 1800 px/s, one priming dab and two four-second trials per API.
Active presentation averaged 59.51–60.23 fps. Worst pointer-sample-to-frame
acknowledgement p95 was 14.68 ms, p99 17.83 ms; worst frame-period p99 was
27.56 ms. Pointer p95 meets the existing one-frame criterion; tails are retained,
and these results do not claim physical pen latency or every sample under a frame.
Evidence: `bench/results/p6-m1-brush-performance.json`. Reproduce serially with
`PERF_PHASE=p6-m1` and the existing viewport/brush scripts; their filenames differ
from the dedicated adjustment report.

## Verification

- Rust formatting, native and WASM clippy, all 87 Rust tests, TypeScript, ESLint,
  Prettier, all 16 Vitest tests and the unchanged 450-expression license inventory
  pass. Eight new Rust tests include independent vectors, exact alpha/coverage,
  tile seams, cold source/selection hydration, refusal and COW undo/redo.
- Release WASM: 3,269,329 bytes; gzip level 9: 1,244,718 / 1,500,000 bytes.
  The timing archive records the WASM hash; later test/UI cache follow-ups leave
  the processing kernels and worker unchanged.
- All 156 local installed Chrome/Edge regression cases passed on WebGPU/WebGL2,
  with no failures, skips or retries. The final UI worker-cache guard additionally
  passed all 12 adjustment cases on those same four hardware paths. Four explicit
  backend/rendered-sample probes also passed; every requested API was present.
- The full local 113-case software suite passed without failures, skips or retries.
  The final cache guard also passed all nine software adjustment cases. Software
  proves function only; it is not part of the hardware timing evidence.
- All 12 final deployed adjustment cases passed on installed Chrome/Edge and both
  APIs, including device-loss/retry and fresh-worker histogram replacement.
- The final Windows executable builds and launches in WebView2 155.0.4283.62 on
  both hardware APIs. Adjustments, exact undo/redo and histogram sums pass, along
  with the existing image/brush/selection/clipboard, streamed layer projects,
  recovery and all GPU blend references (maximum error 1/channel).

The new test assertions were corrected to use the exact alpha sum (1539 × 128)
and to await the reopened document's next UI snapshot. A final worker-replacement
test covers cached histogram IDs/revisions being reused by a fresh engine; caches
are now associated with the API instance as well as the source key. The desktop
harness waits for recovery catalog completion before dismissing a possible
startup dialog. Benchmark report filenames are distinct from the existing brush
harness's output.

The first completed remote run was green with one case retry: automatic recovery
had started between two test RPCs, so the history request correctly refused busy
admission. The test driver now waits through only pre-edit admission refusals,
keeps each edit in its own RPC and explicitly exercises saving beside history
navigation. Processing errors still fail immediately. Final remote statistics
are recorded below.

A later software run reached the existing overlap brush case's timeout: native
three-step mouse moves took up to 73 seconds while low-flow stationary spraying
continued. The software-only overlap fixture now dispatches its fixed 15 pointer
samples and release in one task. Hardware input, separate native-input/airbrush
cases, and every pixel/history assertion remain. Nine repeated software runs of
that case pass. This changes functional test input scheduling, not a timeout or
the application, and makes no software performance claim.

The final [CI run 40091443027](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40091443027)
passes every configured gate, including all 113 software browser cases with zero
failures, skips or flaky retries, the Windows build and both software WebView2
backends. CI WASM is 3,270,618 bytes / gzip9 1,244,781 bytes, below the same limit.
Its `phase-verification` artifact retains the raw browser/desktop/size reports.
[Deployment 40091443033](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40091443033)
also passes. Final deployed hardware cases validate the unchanged application;
the last follow-ups adjust test input scheduling only. The live editor is
[CashGear FotoEditor](https://proud-pebble-07b8b4f10.2.azurestaticapps.net).

Local completion evidence is `bench/results/p6-m1-final-verification.json`, with
links, exact counts, hardware/software separation and earlier-run outcomes. The
raw final CI artifact is also retained under `bench/results/p6-m1-ci-final/`.
M1 is accepted; the complete P6 phase and its tag remain pending.

## Deferred P6 work and next milestone

M2 adds editable adjustment layers and the compositor/persistence changes they
need, CPU/GPU parity within ±1/channel, and visible-viewport GPU live preview.
The ≤100 ms slider-preview criterion remains unverified and unclaimed in M1.

M3 adds Gaussian/box blur, unsharp mask, sharpen and median, tiled neighbor padding,
preview and Fade Last Filter. Gaussian radius 20 on 6000×4000 in under 2 seconds
single-threaded remains a pending acceptance criterion; the Exposure timings above
do not satisfy it. M4 covers the remaining seven adjustments and five filters,
then the complete P6 gates. Smart filters stay outside v1 by user choice.

The P5 Photoshop feather comparison is still pending, without a waiver or invented
reference. Physical pen validation retains its earlier approved deferral. No P7
transform work has started. Proposed next milestone: P6 M2; proposed next phase
after all P6 acceptance passes: P7 transforms. Stop at this M1 boundary.
