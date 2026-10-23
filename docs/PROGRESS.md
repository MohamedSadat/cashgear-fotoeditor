# Progress

## P4 — In progress

Started on 2026-10-23 on clean main at `ae46c5b`, the tagged P3 completion.
Read AGENTS.md, the phase records and original roadmap printed page 13. The user
confirmed sRGB, default pass-through groups, after-stroke thumbnails, and the real
50-dense-layer workload with paging and a 4 GiB streamed project cap. CPU/GPU and
image-file budgets are unchanged. P4-DESIGN.md records the implementation and
acceptance plan. P4 is not complete and no completion tag has been created.

Foundation checkpoint (implementation through `48377d6`):

- Pageable immutable tile-generation handles now back the existing document,
  brush and history surfaces. Pins prevent eviction; validated immutable backing
  is required before eviction. Undo can restore cold handles without retaining
  their buffers. Hydration preserves generation identity and exact hidden RGB.
- Core document data is a bottom-to-top raster/group tree, with masks, properties,
  locks and stable runtime layer IDs. Metadata operations use COW command snapshots
  sharing surface maps; duplicate samples count once. RGBA and Gray8 generations
  share history accounting. A 500-case random layer-command property test verifies
  undo-all/redo restoration, alongside the existing command property tests.
- Per-layer brush targeting and alpha/pixel lock foundations preserve transparent
  RGB and partial alpha. These core APIs are not exposed as layer controls yet.
- Sixteen sRGB reference kernels and a tile-sized CPU composition plan cover
  isolated/pass-through groups, masks and clipping. Portable WGSL parses and
  validates in Naga. GPU execution/readback comparisons remain unfinished.
- V2 stream framing and a layer-aware document adapter preserve properties,
  masks/groups, shared tile references, exact f64 metadata and straight samples.
  Parsing constructs cold tile handles rather than materializing the whole image.
  Tile hydration validates CRC/length; complete stream and semantic validation
  precede returning a candidate. No live document replacement occurs in the codec.
- Project packets and input chunks are bounded. The manifest is capped at 1 MiB
  with a reference-count guard before construction, and project jobs conservatively
  reserve 16 MiB for metadata/framing/bridge scratch. Total framing is included
  in the approved 4 GiB stream cap. Existing v1 compatibility tests still pass.

Local code checks passed: formatting, native/WASM clippy, 55 Rust tests, TypeScript,
ESLint, 14 Vitest tests, production build and all 450 dependency license expressions.
The source graph reuses already inventoried serde/serde_json; float_roundtrip is
enabled for exact f64 project metadata. Optimized WASM is 2,740,614 bytes, gzip9
1,048,341 / 1,500,000 bytes. All 92 installed Chrome/Edge and 65 local software
browser regressions passed without skips, retries or failures on the frozen build.
Windows/WebView2 155.0.4283.62 build/launch, painting, exact undo/redo, byte-identical
v1 project and autosave restore passed on both hardware backends. Save/open picker
and OPFS APIs are exposed in WebView2; actual large streamed file flows remain open.
Local evidence: bench/results/p4-hardware-browser-tests.json,
p4-software-browser-tests.json and p4-desktop-smoke.json. These cover existing
single-raster behavior; they do not establish layer UI/GPU/storage acceptance.

Single-layer hardware regression measurements used Windows 10 Pro 10.0.19045,
Xeon E5-2680 v3, approximately 32 GiB RAM, GTX 1650 / driver 27.21.14.5751,
Chrome 156.0.8078.47 and Edge 155.0.4283.62, 1280×850 / DPR 1, NVIDIA WebGPU and
ANGLE D3D11 WebGL2. The unchanged 6000×4000 / 67,038,649-byte fixture has SHA-256
809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5.
Two four-second warmed 300 px soft-brush trials after a priming dab measured
59.17–60.07 fps and p95 source-to-presentation-ack latency 11.73–15.74 ms;
worst p99 was 18.40 ms. These exclude compositor/scanout and physical pen latency,
and do not involve 200 retained steps. Logical capacities were 96 MiB document,
1–3.5 MiB history, 6.77 MiB stroke scratch and 32.03 MiB GPU atlas/instances;
map/Arc metadata, allocator retention and browser/driver memory are excluded.

First browser-session image opens measured 1.20–1.27 s, subsequent opens
1.01–1.19 s. OS/file caches were not flushed. Pan/zoom/mip-transition trials ran
at 59.998–60.039 fps, worst period p95/p99 17.28/19.23 ms. Cold-mip final refinement
after navigation stopped took 1.90–2.39 s while retaining the cached preview;
this is slower than P3's measured 1.22–1.97 s range and remains recorded explicitly.
These are single-layer regression results, not the P4 fifty-layer acceptance.
Raw data: bench/results/p4-foundation-performance.json,
p4-foundation-viewport-performance.json and p4-foundation-machine.json.

[FotoEditor checks run 39878818706](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39878818706)
and [Azure deployment run 39878818582](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39878818582)
passed for `48377d65af2a830d881ba006122be284d7bab836`. The downloaded CI artifact
contains all 65 eventually passing software browser cases, zero skips/final
failures/global errors, and **three startup retries** in the software-WebGL2
project. Crash recovery/corrupt-latest, timer/tab-hide autosave, and active-writer
tests initially exceeded the 30-second Engine ready deadline; the first two
screenshots show the graphics-engine startup timeout. Each passed on its existing
CI retry. This is not a retry-free remote run and does not establish a storage
failure or a hardware performance result. Traces/screenshots are retained under
bench/results/p4-ci/test-results.

CI also built and launched WebView2 155.0.4283.62 and passed painting/exact undo,
v1 project roundtrip and autosave restore on SwiftShader WebGPU (vendor google)
and WebGL2 Microsoft Basic Render Driver/WARP. Those are software functional
checks only. CI WASM is 2,741,881 bytes, gzip9 1,048,449 / 1,500,000. Its artifact
is retained under bench/results/p4-ci. No P4 completion acceptance, tag or dense
layer performance result is claimed by this foundation checkpoint.

Outstanding within P4 (not deferred to P5): worker OPFS tile backing and eviction,
streamed manual project and recovery integration, GPU compositor execution and
CPU/GPU golden readbacks, incremental composite caching, layer controls/selection/
drag reorder, mask painting/view/apply, merge operations and thumbnails. The live
application still exposes its single-raster controls and v1 128 MiB project path;
the approved 4 GiB layered format is a Rust adapter, not a shipped UI feature yet.
The actual distinct dense 50-layer workload and its cold/warm hardware measurements
remain unverified. No sparse/shared fixture substitutes for that acceptance target.

## P3 — Complete

Completed on 2026-10-23, starting on clean main at `9a0d577`, the accepted P2 handoff.
Read AGENTS.md, the phase documents, and original roadmap printed pages 12–13.
All four user choices are confirmed in DECISIONS.md: 256 MiB / 200 history steps
within the shared 768 MiB CPU model, 30-second plus best-effort tab-hide autosave,
ask before restoring, and document-only files with fresh undo history on reopen.

Shipped:

- History panel with click-to-jump, branching, persistent configurable limits and
  visible eviction counts. Oldest applied commands and then farthest redo states
  give way to the configured limits. Save/readback reservations can also evict
  history before allocating buffers within the total CPU limit.
- Resolution metadata commands merge within one focus/pointer gesture. Strokes
  remain separate. A queued main-thread stroke stays visibly busy until it reaches
  the worker, extending the worker queue protection from P2.
- Version-1 `.cgfoto` sparse project save/open and drag/drop. Rust preserves exact
  straight RGBA8, hidden RGB and f64 DPI; validates versions, lengths, dimensions,
  coordinates and CRC32; and commits a candidate only after complete validation.
  The existing 128 MiB file cap also applies to these uncompressed project files.
- Worker-owned OPFS recovery with unique generation files, explicit flush/close,
  Rust readback validation and a separately flushed metadata commit record.
  Three validated generations per session are retained. Inactive uncommitted files
  are cleaned up. A failed write removes only its own new files and preserves completed saves.
- Session ownership and catalog Web Locks coordinate tabs, exclude active sessions
  from restore prompts and recheck restore/discard operations. Persistent storage
  is requested on the main thread. Quota, write and persistence failures are visible.
- Bounded project batches yield via MessageChannel; each Rust step handles one tile.
  New files are validated once, then two prior valid generations are checked before
  pruning. This removed duplicated whole-file reads and nested timer clamping.

Local acceptance checks: formatting, native/WASM clippy, TypeScript, ESLint, 14 Vitest
tests, all 450 dependency license expressions and production build passed. All
30 Rust tests passed. Coverage includes 500 proptest mixed command sequences with exact undo-all hashes,
200 soft strokes under the budgets, branching/merging/eviction and malformed files.
All 92 installed Chrome/Edge cases passed after the bounded job optimization,
without retries or skips. Windows/WebView2 155.0.4283.62 release build, exact
project roundtrip and autosave/restore checks passed on both hardware backends.
All 65 local software Chromium cases also passed without retries or skips. These
are functional evidence only.
Release WASM: 2,714,673 bytes; gzip level 9: 1,035,102 / 1,500,000 bytes.

[FotoEditor checks run 39870622824](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39870622824)
passed for implementation/verification commit `64a36a4bc6a0700450800c7e318b5c1a80708ee5`.
Every configured Rust/JavaScript/license/build check passed. The artifact confirms
all 65 software browser cases passed on their first attempt: zero skips, retries,
flaky cases, unexpected failures or global errors. P2's earlier software cold-start
retries remain documented in its own report below.

CI also built and launched Windows/WebView2 154.0.4258.62 on both software backends,
verifying painting, exact undo/redo presentation, byte-identical project roundtrip
and autosave restoration after reload. WebGPU used SwiftShader (vendor google);
WebGL2 reported Microsoft Basic Render Driver/WARP. These prove function only.
CI release WASM: 2,715,921 bytes; gzip level 9: 1,035,287 / 1,500,000 bytes.
Its phase-verification artifact is retained locally in bench/results/p3-ci.

Hardware measurements used the same Windows 10 Pro / Xeon E5-2680 v3 / approximately
32 GB RAM / GTX 1650 / driver 27.21.14.5751 machine as P2. Installed headless Chrome
156.0.8078.47 and Edge 155.0.4283.62, viewport 1280×850, DPR 1. WebGPU reports nvidia /
turing / isFallbackAdapter false; actual WebGL2 adapter strings identify ANGLE /
NVIDIA GeForce GTX 1650 / Direct3D11. The unchanged 6000×4000 PNG fixture is
67,038,649 bytes, SHA-256 809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5.

On every hardware path, 200 separate 300 px soft dabs retained all 200 commands:
96 MiB document plus 214 MiB history, 310 MiB post-stroke logical CPU tile buffers.
This is below the 256 MiB history cap and 768 MiB total admission limit; transient
scratch is tested separately and file/bridge buffers are admitted before allocation.
Logical capacities exclude map/Arc headers, retained allocator pages and browser/
driver overhead; these are not exact process RAM/VRAM measurements.

Two warmed save/reopen cycles followed those strokes. Each project is 100,666,408
bytes; all eight saved files have identical SHA-256
609726aee71e5d10c4fea1f3fc83023761daa71f84271733c087e1507414f0ca.

| Hardware path   | Project save trials | Project reopen trials | Autosave worker range |
| --------------- | ------------------- | --------------------- | --------------------- |
| Chrome / WebGPU | 1.63 / 1.40 s       | 0.86 / 0.85 s         | 1.14–1.68 s           |
| Chrome / WebGL2 | 1.42 / 1.73 s       | 0.85 / 0.85 s         | 1.07–1.62 s           |
| Edge / WebGPU   | 1.49 / 1.46 s       | 0.87 / 0.89 s         | 1.18–1.73 s           |
| Edge / WebGL2   | 1.50 / 1.41 s       | 0.87 / 0.88 s         | 1.14–1.73 s           |

Save/reopen timings include UI automation, input/download I/O, bridge copies and
observation polling. They measure document commit/normal frame acknowledgement,
not compositor/scanout or completion of every preview refinement. Four manual
autosaves per path measured worker serialization, flush, readback, metadata commit
and pruning; UI observation was 1.93–1.96 s. Autosave currently pauses editing for
its bounded operation; the 1.07–1.73 s worker cost on this workload is disclosed.
Three completed generations remain after the fourth save. The initial redundant
validation implementation observed approximately 3–5 s; its raw baseline is retained.

The separate P2 brush workload regression held 59.79–60.09 average fps, with p95
source-sample-to-presentation-acknowledgement 9.43–14.91 ms. Worst sample p99 was
16.67 ms; worst acknowledgement cadence p95/p99 was 22.71/26.45 ms. This uses the
same two four-second warm 300 px soft-brush trials, native 120 Hz mouse input and
1800 px/s path. It includes queueing, CPU paint, conversion/upload and submission,
and excludes compositor/scanout and physical pen latency. It is not a hard frame
deadline or a measurement of continuous painting with 200 retained history steps.

Viewport regression: first image opens 1.10–1.38 s, warm opens 0.98–1.30 s;
approximately 60 fps for pan, zoom and cold-mip zoom; worst p95/p99 period
16.74/16.78 ms. Final cold-mip refinement took 1.22–1.97 s with the cached overview.
Raw evidence is in ignored bench/results/p3-history-performance.json,
p3-history-performance-initial.json, p3-performance.json, p3-viewport-performance.json,
p3-machine.json and p3-*.png. Reproduce with pnpm bench:history. For regression
measurements set $env:PERF_PHASE = 'p3' in PowerShell, then run pnpm bench:brush
and pnpm bench:viewport.

[Azure deployment run 39870622875](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39870622875)
passed for implementation/verification commit `64a36a4bc6a0700450800c7e318b5c1a80708ee5`.
The public [CashGear FotoEditor](https://proud-pebble-07b8b4f10.2.azurestaticapps.net)
then passed all 92 installed Chrome/Edge cases on WebGPU and WebGL2, without
retries or skips. This includes actual renderer kills during an uncommitted write,
fallback from a damaged snapshot, byte-identical projects, writer coordination,
storage failures, isolation headers and WASM delivery. Evidence is in
bench/results/p3-live-browser-tests.json. Local hardware/software and desktop
evidence is in p3-hardware-browser-tests.json, p3-software-browser-tests.json and
p3-desktop-smoke.json.

No P3 acceptance items are deferred. Intentional v1 limits are document-only
uncompressed projects capped at 128 MiB, local browser/profile recovery, and the
bounded editing pause during save/recovery work. Restored source archives remain
until Discard recovery; browser storage clearing/eviction and OS power-loss
durability are outside the tested renderer-crash contract. Physical pen validation
remains the explicitly approved P2 deferral; image tips/textures remain P9.

The p3-history tag includes this documentation-only completion report after the
verified implementation. All required acceptance checks, remote CI and deployment
are green. Proposed next phase: P4 — layers, groups, masks and compositing, extending
the project format for layer data within the residency model. Stop at this boundary;
P4 has not started.

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
