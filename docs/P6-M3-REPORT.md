# P6 M3 - Spatial filters and Fade

Work began on 2026-10-24 from synchronized main at `77ec974`, following the
user's fresh-session request. Existing P6 decisions and the complete original
roadmap section on printed pages 14-15 apply. Work remains directly on main,
without agents or a worktree. Implementation and local acceptance are complete.
Pushed CI/deployment remain the final milestone gates. M4 is not started;
M3 does not authorize the `p6-filters` phase tag.

## Delivered behavior

- Gaussian blur, box blur, unsharp mask, sharpen and median operate on the
  active raster's pixels. Every Apply reads immutable input, includes neighbor
  tile halos, clips output through selection coverage and commits one command.
  Undo/redo restores exact samples, including hidden RGB. Locks, cancellation,
  no-ops, budget refusal and streamed project persistence use existing behavior.
- Live filter preview runs on the GPU over the visible source cache and the
  complete layer compositor. It is opt-in and temporary. Controls coalesce
  intermediate requests; the adjustment and filter panels share ownership.
  Apply uses the complete selected area at document resolution. Export and
  recovery read committed samples. Changing view, target, document or worker
  invalidates temporary preview state.
- Fade Last Filter uses opacity and all 16 existing blend modes. Repeated edits
  recompute from immutable pre-filter, full-filter and selection-clipped
  endpoints. Normal at zero restores the exact original and at one restores
  the exact filtered result. Each Fade is undoable. Another document edit,
  history navigation or target change ends availability; save/export/view
  navigation retain it.

## Processing and resources

The specification is recorded in P6-DESIGN.md. Gaussian uses normalized finite
exponential weights over plus/minus radius, sigma radius/3, with two separable
passes. It is not a box approximation. Convolution/median use premultiplied
floating-point sRGB temporaries, clamp canvas edges and sample unselected halo
pixels. Selection coverage is applied once before straight-alpha quantization.
Output whose alpha rounds to zero retains the original hidden RGB. Unsharp and
sharpen preserve original alpha; alpha lock also preserves it for other filters.
Median takes the middle value independently per premultiplied component.

Convolution radii are 0-64; median radii are 0-5; sharpen amount is 0-5 and
unsharp threshold is 0-255. Radius zero is identity. WASM SIMD uses symmetric
Gaussian taps, four accumulators and narrow column batches. Proven opaque blur
neighborhoods gather packed RGB directly; independent references exercise this
path and the general alpha path. Processing stays in Rust on one engine worker.

Each job reuses bounded scratch, reserved at eight MiB. Immutable source,
candidate, Fade and history generations share the existing residency accounting
and pager. A temporary allocator reservation of at most 16 MiB at 32-tile
boundaries batches WASM heap growth; it is charged to remaining CPU room and
freed before the tile operation. Logical CPU and GPU budgets remain 768 MiB and
128 MiB, image files 128 MiB and streamed projects 4 GiB. These are buffer
accounting limits, excluding retained allocator pages and browser/driver overhead.
No dependency or license was added.

Preview samples a maximum 1920-pixel interior per side, with a 64-sample halo
and no more than roughly one sample per document pixel when zoomed in. GPU
scratch includes a 384-square RGBA8 source, 384-square RGBA32F horizontal
target, 256-square RGBA8 output, selection tile and uniforms. It replaces the
active raster's compositor source before masks/blends/upper layers. Median's
16 bisections use exactly represented byte premultiplied products. Filter
radii scale and round to sampled pixels; fitted/rotated preview is an
interactive approximation. Actual-pixel comparisons and full-resolution Apply
have separate correctness checks. Initial preparation includes source-cache
work and the GPU queue fence; warm slider updates reuse those resources.

## Verification and performance

All 107 Rust tests, native/WASM clippy, Rust/JS formatting, TypeScript, ESLint,
16 Vitest cases and the 450-expression license inventory pass. The final 16
installed-browser feature cases pass without failures, retries or skips. They
compare 56 complete GPU tiles with Rust references per browser/backend within
one channel value, including opaque SIMD input, low/zero alpha, selections,
alpha lock, odd canvas boundaries, radius-zero identity and maximum radii.
Independent float64 Gaussian tests cover canvas and tile edges. Browser cases
also cover exact undo/redo, repeated Fade, persistence, atomic cancellation,
mid-operation budget refusal and temporary/coalesced UI preview.

The full installed Chrome/Edge hardware regression passes all 204 cases, without
failures, skips or retries. The native Windows shell builds against the same
frozen distribution and both hardware WebView2 APIs pass the expanded smoke:
all five previews/filters, exact undo, Fade endpoints and streamed persistence.
WebView2 Runtime is 155.0.4283.62 with NVIDIA hardware verified. The full software
regression passes all 149 cases without failures, skips or retries. The two
presentation/admission corrections also pass 18 repeated software cases and all
eight installed-browser hardware follow-ups. Completion requires green CI and
deployment for the pushed head and inspection of their verification artifacts.
The task's completion message records immutable run links. No phase completion
tag is authorized.

Local release WASM is 3,485,650 bytes / gzip9 1,318,701 bytes, below 1,500,000.
The frozen measured distribution has SHA-256
`faee92a2341085a363057aec120da833ec1ce2d455028cb63a20c9df1bdf4f3e`.
Hardware measurements ran serially, without competing builds or GPU tests, on
Windows 10 Pro, Xeon E5-2680 v3 at 2.50 GHz, about 32 GiB RAM and NVIDIA GTX 1650. Chrome is 156.0.8078.47; Edge is 156.0.4314.42. The WebGPU adapter is
verified non-fallback NVIDIA; WebGL2 reports NVIDIA D3D11 ANGLE. Viewport is
1280 by 850, DPR 1.

The resident unselected 6000 by 4000 NASA/JPL-Caltech/IPGP photo comes from
the official Setting the Wind and Thermal Shield image, cropped in Rust from
6002 by 4000 at [1,0,6000,4000], without resampling. PNG SHA-256 is
`603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`.
Each backend has one first application plus five warm applications from the
original pixels. Undo presentation and refinement must finish before each
timer. No output cache is used. API return includes the committed command;
visible time additionally waits for its viewport frame. Histogram independently
visits all 24 million pixels after each trial. This is resident single-worker
processing, not a disk-cold paging test. With five warm samples, p95 is the
largest sample.

| Browser / API | First apply | Warm apply p50 / p95 | Warm visible p95 |
| ------------- | ----------: | -------------------: | ---------------: |
| Chrome WebGPU |     1.247 s |      1.282 / 1.386 s |          1.550 s |
| Chrome WebGL2 |     1.298 s |      1.264 / 1.389 s |          1.564 s |
| Edge WebGPU   |     1.259 s |      1.286 / 1.314 s |          1.467 s |
| Edge WebGL2   |     1.291 s |      1.244 / 1.316 s |          1.500 s |

All 24 Gaussian radius-20 applications pass the two-second gate. Logical CPU
peak including the allocator reservation is 304 MiB, GPU 16.11 MiB. Separate
histogram warm p95 is at most 0.677 seconds.

Preview measurements use the fitted resident photo and five filter controls.
Twenty real DOM slider changes per control per backend measure input through
completed React status and the following UI frame: 400 events total. All are
below 100 ms, with worst p95 47.82 ms and maximum 63.19 ms. Separately, ten warm
API trials per filter/backend have worst p95 34.08 ms. Initial Gaussian cache
preparation plus first update is 0.50-0.55 seconds on WebGPU and 1.35-1.42
seconds on WebGL2, including first shader/queue initialization; this is excluded
from warm slider latency and remains visible as preparation. Other controls
also record their first preparation separately. Logical preview residency peaks
at 105.063 MiB CPU / 26.616 MiB GPU. These measurements establish the specified
workload, not every layer count, alpha distribution, zoom or radius. All four
preview screenshots were visually inspected.

An additional actual-pixel run exercises nonzero sharpening neighborhoods and
maximum convolution radius 64 / median radius 5 API cases on all four hardware
pairs. Its 400 DOM events have worst p95 48.48 ms and maximum 64.50 ms; warm API
p95 is at most 38.42 ms. Initial Gaussian preparation plus first update is
0.38-0.45 seconds on WebGPU and 1.27-1.40 seconds on WebGL2. Logical residency
peaks at 105.063 MiB CPU / 26.508 MiB GPU. The combined 800 fitted/actual DOM
events all pass 100 ms. Representative actual-pixel screenshots were inspected.
Set `PERF_VIEW` to `actual` for this additional benchmark; its raw record is
`bench/results/p6-m3-preview-actual-performance.json`.

Raw records and reproducible scripts: `bench/results/p6-m3-filter-performance.json`
(`pnpm bench:filters`), `p6-m3-preview-performance.json`
(`pnpm bench:filter-preview`), and `p6-m3-feature-hardware.json`. The distribution
remains frozen throughout browser verification. Software checks supply functional
evidence only and are not included in these hardware timings.

Superseded failures remain evidence, not acceptance results. Initial 24 MP
Gaussian application took about 3.1-4.0 seconds. A first-warm-call spike was
traced to 339 WASM heap growth calls taking 1.31 seconds; bounded allocator
reservations removed repeated growth. A later Edge WebGPU warm call still
took 2.17 seconds, motivating direct opaque RGB gathering. The median shader
initially produced an unsupported integer-vector GLSL mix on WebGL2; exactly
represented float lanes corrected the portability issue. The preview test's
history baseline originally read a stale presented snapshot; it now uses the
acknowledged fixture command result.

The first full software run passed 147 cases and failed two test timing checks.
After project reopening, the filter test read the previous presented `canFade`
snapshot; it now waits for the reopened presentation. The layer-control test
clicked Export while mask-view recomposition still held worker admission and
then waited for a download that was refused. Its export helper now waits for
idle admission. The trace and failed run remain in the raw evidence. These
corrections retain the existing lock-input wait and brush-delivery fixes.

## Deferred scope and proposed next milestone

M4 completes vibrance, color balance, black & white, invert, threshold,
posterize and gradient map as destructive operations and adjustment layers;
motion blur, high pass, add noise, emboss and pixelate; then phase-wide
validation and reporting. Smart filters remain excluded by the user's decision.
Stop before M4. Do not create `p6-filters` until the entire phase passes.

P5's Photoshop feather comparison is still pending, without `p5-select`.
Physical Windows Ink pen validation remains explicitly deferred. Synthetic
tests do not substitute for either reference or physical evidence.
