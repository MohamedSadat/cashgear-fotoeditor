# Progress

## P2 — Complete

Completed on 2026-10-22 from the accepted p1-viewport tag, directly on main.
Read the original roadmap's detailed P2 requirements (printed pages 11–12) and
the approved corrections. All product questions are answered in DECISIONS.md.
The command/basic undo foundation was committed and verified before painting.

Shipped:

- General apply/revert commands with COW before/after tile generations, exact
  sparse and hidden-RGB restoration, branch-safe undo/redo, and atomic memory
  admission. No-op/rejected commands preserve redo. Each completed stroke is one
  command; new/open replacements reset history.
- Rust procedural elliptical dabs with size, hardness, opacity, flow, spacing,
  roundness and angle; kurbo line interpolation; hard/soft round, stationary
  airbrush and one-document-pixel pencil presets.
- Stroke coverage accumulates flow while opacity caps the complete stroke.
  Straight-alpha paint preserves source-over color; erasure preserves hidden RGB.
  Bounded COW preview/mask/tip/input buffers are admitted within the existing
  768 MiB CPU budget, including a 1 MiB display reserve. GPU remains capped at
  128 MiB. Overflow/refused strokes cancel without changing document/history.
- Editable five-point pressure/tilt curves for size/opacity, both pulled-string
  and weighted-average stabilizers, coalesced pen samples and raw-pointer input
  with duplicate suppression. Mouse input uses pressure one. Physical Windows Ink
  validation is explicitly deferred by user approval because no pen is available.
- HSV square/hue slider, RGBA hex, primary/secondary swapping, recent colors,
  eyedropper and eraser; B/E/I/[ / ]/X/Alt and basic undo shortcuts. Escape, focus
  loss and lost capture cancel active strokes. Narrow windows use a brush drawer.
- True-size dynamic brush outline, live dirty-region uploads, generation/mip
  invalidation and neighboring gutter refresh. SIMD/hot-loop boundaries optimize
  the Rust kernels. The sole frame pump prioritizes input before requesting a
  later unchanged preview; idle documents submit no GPU frames.

Local acceptance verification: Rust formatting, native/WASM clippy, 23 Cargo tests,
TypeScript, ESLint, Prettier, 13 Vitest tests and all 442 dependency license
expressions passed. Release WASM is 2,689,780 bytes; gzip level 9 is
1,024,905 / 1,500,000 bytes. All 64 installed Chrome/Edge tests passed on WebGPU
and WebGL2 without retries; all 44 software browser tests also passed. The Windows
desktop release build and WebView2 155.0.4283.62 launch/isolation checks passed on
both hardware backends, including actual painted pixels and complete canvas pixel
comparisons after undo/redo. The final desktop package was rebuilt and rechecked
after the queued-stroke fix.

[FotoEditor checks run 39840183893](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39840183893)
passed for implementation commit `596dba30e8fd1620f3b15c7adea269bfefdf38bf`.
Every configured Rust/JavaScript/license/build check passed, and all 44 software
browser cases ultimately passed with zero skips or unexpected final failures.
Three cases in the separate software WebGL2 project needed the existing CI retry:
initial engine startup exceeded the 30-second deadline. Diagnostic pixels,
resize/recovery and PNG navigation all passed on their next attempt. These were
startup timeouts before editing, not painting assertions. They remain a known
CI stability limitation; the report does not claim a retry-free remote run.
Local software, installed hardware and live-site suites passed without retries.

CI also built and launched Windows/WebView2 on both backends and verified painting
plus exact presented undo/redo pixels. Its WebGPU adapter was SwiftShader and
WebGL2 used Microsoft Basic Render Driver/WARP; these prove function only.
CI release WASM measured 2,691,028 bytes, gzip level 9: 1,025,004 bytes, below
the same 1,500,000-byte gate. The phase-verification artifact retains browser,
desktop, size and failure/retry evidence; a local copy is in bench/results/p2-ci.

[Azure deployment run 39840183890](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39840183890)
passed for implementation commit `596dba30e8fd1620f3b15c7adea269bfefdf38bf`.
The public [CashGear FotoEditor](https://proud-pebble-07b8b4f10.2.azurestaticapps.net)
then passed all 64 tests on installed Chrome and Edge using WebGPU and WebGL2,
with no retries or skips. This covers actual presented pixels, exact exported
samples/undo/redo, queued input, synthetic pen dynamics, isolation headers and
WASM delivery. Evidence is in bench/results/p2-live-browser-tests.json.

Final review also closed a lifecycle race between a completed stroke and the next
queued stroke: the worker now reports the complete queued edit as busy, preventing
open/export from interleaving at the command boundary. All 24 hardware brush cases
passed again. The additional queued-stroke regression passed on all four hardware
paths and all three software test configurations. The test observes the intermediate
history state during a long stroke followed immediately by another color stroke.

Hardware brush measurements used Windows 10 Pro 10.0.19045, Xeon E5-2680 v3
2.50 GHz, approximately 32 GB RAM, GTX 1650, driver 27.21.14.5751. Both browsers
reported vendor nvidia, architecture turing, isFallbackAdapter false. Viewport
1280×850, DPR 1, installed headless Chrome 156.0.8078.47 / Edge 155.0.4283.62.
The existing deterministic 6000×4000 RGBA8 PNG is 67,038,649 bytes, SHA-256
809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5.
Two four-second warm strokes follow a priming dab: 300 px soft round, 100% view,
10% spacing, 1800 px/s triangular path with vertical motion, native protocol
mouse input at 120 Hz. Raw input was available on all four configurations.

| Browser / backend | Trial fps     | Sample-to-ack p95 | Worst sample p99 | Worst cadence p95 / p99 |
| ----------------- | ------------- | ----------------- | ---------------- | ----------------------- |
| Chrome / WebGPU   | 60.11 / 59.99 | 15.06 / 14.72 ms  | 17.14 ms         | 22.94 / 25.02 ms        |
| Chrome / WebGL2   | 60.15 / 60.00 | 15.17 / 15.37 ms  | 17.34 ms         | 22.69 / 25.58 ms        |
| Edge / WebGPU     | 60.09 / 59.74 | 14.99 / 14.59 ms  | 16.93 ms         | 23.16 / 25.56 ms        |
| Edge / WebGL2     | 60.14 / 60.07 | 15.39 / 14.84 ms  | 17.07 ms         | 22.79 / 26.22 ms        |

Source-sample latency starts at the input first changing a presented document
pixel and ends at main-thread observation of the worker's presentation
acknowledgement. It includes queueing, paint, display conversion/upload and GPU
submission; it excludes compositor/scanout and physical input latency. The p95
result is below a 16.67 ms frame; p99 outliers are shown rather than claiming a
hard real-time bound. Full-dab completion p95 was 14.72–15.32 ms. Average active
presentation was approximately 60 fps, including previews between spaced dabs;
acknowledgement cadence is not a promise that every interval is 16.67 ms.

Paint batches p95 were 4.37–4.66 ms; render p95 2.25–2.85 ms. Pending work peaked
at two samples and returned to zero by each trial's last observed presentation.
Dense document tiles used 96 MiB, history
1–3.5 MiB, stroke scratch plus display reserve 6.77 MiB, GPU atlas/instances
32.03 MiB. These are logical buffer capacities, excluding map/Arc headers,
browser/driver overhead and retained WASM allocator pages, not process RAM/VRAM.
Raw evidence: bench/results/p2-performance.json, p2-machine.json, p2-*.png and
p2-hardware-browser-tests.json. Reproduce with pnpm bench:brush.
These final measurements used implementation commit
`596dba30e8fd1620f3b15c7adea269bfefdf38bf`. The preceding verification run is
retained in p2-performance-04e0849.json: 59.85–60.14 fps and p95 14.76–15.76 ms.

The viewport performance regression also passed on all four hardware paths:
first opens 1.14–1.20 seconds, warm opens 0.97–1.10 seconds, approximately 60 fps
for pan/zoom/cold-mip zoom, worst p95/p99 interval 16.73/16.78 ms. Final cold-mip
refinement took 1.43–1.82 seconds while showing the cached preview. Evidence is
bench/results/p2-viewport-regression.json; the earlier P1 baseline is preserved.
Software browser results are in p2-software-browser-tests.json and desktop results
in p2-desktop-smoke.json. Software rendering is functional evidence only.

Deferred: physical pen/Windows Ink validation (explicitly approved), image tips
and textures (P9), history panel/budget eviction/project files/recovery (P3).
Software cold-start timing, physical input-to-photon latency, broader hardware
and exact process/driver memory are not claimed by these measurements.

The p2-brush tag includes this documentation-only completion report after the
verified implementation. All required checks completed and remote CI/deployment
are green, with the software startup retries and approved pen deferral disclosed.
Proposed next phase: P3 — history controls/budget eviction, versioned project
save/reopen and recoverable autosave. Stop at this boundary; P3 has not started.

## P0 — Complete

Completed on 2026-10-22. The implementation is verified by
[GitHub Actions run 39778490846](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39778490846)
at commit `5f8bc0ddab6080b806da7fb66e03e3eab3ad8ed6`. The completion report is a
documentation-only follow-up; the `p0-scaffold` tag includes it.

Shipped:

- Seven Rust crates, a React/Vite workspace, and a minimal Tauri/WebView2 shell.
- Worker-owned wgpu renderer with actual-adapter detection, WebGL2 fallback,
  physical-pixel sizing, frame backpressure, and fresh-worker recovery.
- Clear canvas, Rust-uploaded texture diagnostics, fault injection, and dark shell.
- Static isolation headers, reproducible toolchain setup, and GitHub Actions.
- Reviewed roadmap decisions and a checked inventory of 431 dependencies.

Local verification on 2026-10-22:

| Check                                       | Result                                                   |
| ------------------------------------------- | -------------------------------------------------------- |
| Rust formatting and native/WASM clippy      | Passed                                                   |
| Cargo workspace tests                       | Passed; diagnostic WGSL validated                        |
| TypeScript, ESLint, Prettier                | Passed                                                   |
| Vitest                                      | 4 passed                                                 |
| Chrome 156.0.8078.47 and Edge 155.0.4283.62 | 24 browser tests passed                                  |
| Software Chromium                           | 14 tests passed; SwiftShader explicitly asserted         |
| Windows/WebView2 155.0.4283.62              | Both backends render; isolation confirmed                |
| Development Rust watch/HMR                  | Rebuild and worker replacement passed                    |
| License inventory                           | 431 dependency expressions approved                      |
| Release WASM                                | 1,912,638 bytes; gzip level 9: 782,613 / 1,500,000 bytes |

Hardware browser and desktop checks used Windows 10 with NVIDIA GeForce GTX 1650
(driver 27.21.14.5751). WebGL2 identifies the NVIDIA adapter; WebGPU intentionally
does not expose a descriptive adapter name through wgpu on this browser. A separate
browser probe confirms vendor `nvidia`, architecture `turing`, and
`isFallbackAdapter: false` in both installed browsers.

Remote verification passed every configured check, all 14 software browser tests,
the Windows desktop build, and WebView2 launch/rendering on both backends with
cross-origin isolation. Browser coverage includes rendered pixels from Rust texture
uploads, fallback after adapter/device failure, DPR changes, resizing, zero-size
pause/resume, disposal, initialization failure, device loss, panic, and retry.
Software browser tests assert SwiftShader; the desktop test uses SwiftShader for
WebGPU and the Microsoft WARP driver for WebGL2. These are functional results,
not hardware performance measurements.

CI's optimized WASM measured 1,913,528 bytes, or 782,698 bytes with gzip level 9,
also below the 1,500,000-byte limit. Both local and CI builds enforce the limit.
CI artifacts retain the size report, browser report, desktop metadata, and desktop
screenshot. The runner-specific WebView2 argument policy is restored after testing.

Known limitations: P0 opens no image and has no editing tools. Installer/updater and
native file integration remain P10 work. Windows 11 and other hardware are not
claimed as tested. Public hosting was added after P0, as recorded below.

The `p0-scaffold` tag marks the validated foundation. There are no known outstanding
P0 acceptance failures.

At P0 completion, no later editing phase had started.

Suggested next phase: P1 — Tiles, viewport, open and PNG export. It establishes
straight-alpha samples and residency accounting before pixel edits begin. The
four outstanding product questions are recorded in PLAN.md.

## Post-P0 — Azure deployment

Published on 2026-10-22 to
[Azure Static Web Apps](https://proud-pebble-07b8b4f10.2.azurestaticapps.net).
[Deployment run 39785836033](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39785836033)
passed for implementation commit `c646a1418cf1f1303080f0869f7eac033e99e459`.
[P0 checks run 39785835868](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39785835868)
also passed for the same implementation, including all software browser tests and
the Windows desktop build and launch.

The workflow now builds release WASM and Vite with the pinned toolchains, then
uploads apps/web/dist. Azure's staticwebapp.config.json supplies isolation headers,
the WASM MIME type, and caching rules. The generated workflow's formatting failure
is corrected.

Live production checks passed on installed Chrome and Edge using both WebGPU and
forced WebGL2: actual diagnostic texture pixels, successful worker/WASM loading,
crossOriginIsolated, all three isolation headers, application/wasm, entry HTML
revalidation, and immutable hashed asset caching. Results and screenshots are
available locally in bench/results/azure-smoke.json and bench/results/azure-*.png.
No image editing phase was started.

## P1 — Complete

Completed on 2026-10-22 directly on main. All four product questions were
answered by the user and recorded in DECISIONS.md. Implementation and verification
commit `89b9a0297ff1abcddaf38f2655a3e5baf359334a` passed remote CI and deployment.
This report is a documentation-only follow-up included in the p1-viewport tag.
No P2 work has started.

Shipped:

- Browser-independent generic Pixel, Rgba8 and Gray8 types, sparse 256×256 tiles,
  copy-on-write identities, dirty tracking and a single-raster document.
- Exact straight-alpha samples, including hidden RGB under zero alpha. Display
  buffers alone premultiply, filter and add neighboring tile gutters. Odd canvas
  edges clamp to valid samples instead of fading into tile padding.
- CPU admission accounting from the start: live document, pending document/job,
  history (zero in P1), codec/conversion reserve and export/bridge copies. A
  768 MiB CPU budget and 128 MiB input/output limits reject oversized jobs.
- Visible-tile GPU atlases with eviction, a pinned fit overview, multiple cached
  mip levels and a 128 MiB logical GPU budget. Cold mips refine in short batches
  while a cached preview remains visible.
- PNG/JPEG/WebP opening in the worker, drag/drop, the confirmed new-canvas presets,
  custom dimensions/DPI, and streaming PNG export with preserved resolution.
  Import construction and export yield and can be cancelled before commit.
- Pointer-anchored 1%–3200% zoom, scroll-first wheel/trackpad handling,
  Space/middle-button pan, R-drag rotation, fit, 100%, rotation reset and view flip.
  Nearest display at >=100%, filtered mips below, and a pixel grid at >=800%.
- Dimension, zoom, cursor, backend and logical residency readouts, errors and retry.
- Reproducible Rust-generated fixtures, hardware performance harness, deployed-URL
  test support, and Windows/WebView2 checks that open an image and verify pixels.

Local acceptance verification:

| Check                                     | Result                                                                          |
| ----------------------------------------- | ------------------------------------------------------------------------------- |
| Rust formatting, native and WASM clippy   | Passed                                                                          |
| Cargo workspace tests                     | 9 passed, including both WGSL shaders                                           |
| TypeScript, ESLint, Prettier              | Passed                                                                          |
| Vitest                                    | 5 passed, including ordered input/backpressure                                  |
| Dependency license inventory              | 439 approved expressions                                                        |
| Chrome 156.0.8078.47 / Edge 155.0.4283.62 | 40 tests passed on WebGPU and WebGL2                                            |
| Software Chromium                         | 26 tests passed; software adapters explicitly identified                        |
| Windows/WebView2 155.0.4283.62            | Build, isolation, both backends and rendered document samples passed            |
| Straight-alpha PNG export                 | Byte-identical decoded RGBA8 samples, including all alpha values and hidden RGB |
| Coordinate round-trips                    | Within 1e-8 document pixels at tested zooms, rotations and flips                |
| Maximum sparse canvas                     | 16,384², zero document pixel allocations                                        |
| Optimized release WASM                    | 2,505,816 bytes; gzip level 9: 1,030,738 / 1,500,000 bytes                      |

An intermittent WebGPU texture validation failure exposed a resize/presentation
race during opening. The operation bar now keeps a fixed height, duplicate canvas
dimension resets were removed, and presentation waits one animation frame after
surface reconfiguration. The frame pump alone presents, including the first opened
image. Thirty repeated opens and the final 40-test hardware suite passed after
the fix. Additional half-alpha checkerboard pixel checks passed on both browsers
and both backends. Software assertions await stable worker state. A CI failure
showed that transient import-memory samples can be missed between rendered frames;
the cancellation test now uses the visible operation, and cancellation also covers
the file read before worker dispatch. The corrected full local software run passed
without retries. The combined software-only I/O/navigation case has a three-minute
functional budget for shared CI CPUs; individual assertion deadlines and the
separate hardware performance criteria are unchanged.

Hardware performance, measured serially on Windows 10 Pro 10.0.19045, Intel Xeon
E5-2680 v3 at 2.50 GHz, approximately 32 GB RAM, NVIDIA GeForce GTX 1650,
driver 27.21.14.5751. Both browsers reported vendor nvidia, architecture turing and
isFallbackAdapter false. Viewport 1280×850, DPR 1, headless installed Chrome/Edge.
The deterministic 6000×4000 RGBA8 PNG is 67,038,649 bytes and is generated by
fotoeditor-io/examples/fixtures.rs. The first open uses a ready fresh worker; two
subsequent opens are warm. Timing includes file reading, decode, tiling, fit atlas
upload and first normal presentation acknowledgement, excluding app/WASM startup.

| Browser / backend | First open | Warm opens    | Pan/zoom/cold-mip zoom | Maximum p95 / p99 interval |
| ----------------- | ---------- | ------------- | ---------------------- | -------------------------- |
| Chrome / WebGPU   | 1.44 s     | 1.14 / 1.18 s | 60.0 fps               | 16.72 / 16.74 ms           |
| Chrome / WebGL2   | 1.36 s     | 1.04 / 1.05 s | 60.0 fps               | 16.71 / 16.73 ms           |
| Edge / WebGPU     | 1.48 s     | 1.05 / 1.00 s | 60.0 fps               | 16.72 / 16.74 ms           |
| Edge / WebGL2     | 1.38 s     | 1.07 / 1.04 s | 60.0 fps               | 16.72 / 16.75 ms           |

Each navigation mode ran for four seconds, approximately 240 presented frame
acknowledgements. Percentiles measure main-thread observation of worker
presentation cadence, not GPU timestamp latency. Warm pan/zoom stayed within a mip
interval; the additional cold-mip zoom crossed multiple levels. After that cold
zoom stopped, final mip refinement took 2.31–2.84 seconds while the cached preview
remained visible. This latency is a remaining display-cache tuning opportunity.
The dense document uses 96 MiB of tile buffers; GPU atlas/instance allocations were
about 16.03 MiB in the warm view and 96.40 MiB across cold mip levels, below the
128 MiB limit. These logical figures exclude browser/driver overhead and retained
WASM allocator pages; they are not process-memory or VRAM measurements.

Local raw evidence is in bench/results/p1-performance.json, p1-machine.json,
p1-*.png, browser-tests.json and desktop-smoke.json. Re-run pnpm bench:viewport
to reproduce hardware measurements. Software checks prove function only.

Deferred by the reviewed phase schedule: pixel editing, general apply/revert
commands and basic undo (P2, before painting); project save/reopen, recovery and
history eviction (P3); layers (P4); extended formats/color management (P8); native
file integration/packaging (P10); 16-bit samples (P11); broader hardware and paging
expansion/performance tuning (P12). Decoder calls themselves remain synchronous;
cancellation resumes between construction chunks. All four P1 product choices
are implemented; these later-phase items do not imply editing support in P1.

Remote verification passed in
[FotoEditor checks run 39803601291](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39803601291),
including every configured Rust/JavaScript check, 26 software browser tests with
zero skips, failures or flaky retries, and the Windows desktop build and image
rendering check on both backends. CI's desktop uses SwiftShader for WebGPU and
Microsoft WARP for WebGL2; these are functional results, not hardware performance.
CI's release WASM measured 2,507,045 bytes, gzip level 9: 1,030,784 bytes, below
the same 1,500,000-byte gate. The phase-verification artifact retains the reports.

[Azure deployment run 39803601290](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39803601290)
also passed. The public build at
[CashGear FotoEditor](https://proud-pebble-07b8b4f10.2.azurestaticapps.net) passed
40 live foundation/viewport tests, followed by all 16 P1 tests after the final
file-read cancellation fix, on installed Chrome and Edge using WebGPU and WebGL2.
This includes actual rendered samples, exact PNG exports, isolation headers and
WASM delivery. The last follow-up changes only the software test time budget.
The existing Azure configuration and custom-domain work remain separate.

The p1-viewport tag marks the accepted phase. There are no known outstanding P1
acceptance failures. Cold mip refinement latency, synchronous decoder calls and
logical rather than process-memory accounting are the limits described above.

Proposed next phase: P2 — establish general commands and
basic undo before adding brushes and color. Stop at the P1 boundary until the user
starts that phase.
