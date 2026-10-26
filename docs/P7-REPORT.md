# P7 - Transforms, crop, resampling and guides

Implementation and local verification completed on 2026-10-27. The release marker
is the annotated `p7-transform` tag, created only after the final commit's remote
CI/deployment and artifact inspection pass. Its annotation records the immutable
head and completion evidence. Work stops before P8.

P7 started on 2026-10-25 after accepted P6 at `5a22595` / `p6-filters`, with the
user's explicit authorization. The original roadmap's complete P7 section on
printed pages 15-16 was read as text and rendered pages. Work stays directly on
main, with small conventional commits and no agents or worktrees. Product choices
remain in DECISIONS.md; processing details are in P7-DESIGN.md.

## Delivered behavior

- Move (`V`) supports a single command per drag, optional layer auto-selection
  through rendered alpha/masks/groups/clipping, and one-pixel arrow nudges
  (Shift: ten). Position/all locks and locked ancestors are respected.
- Free transform (`Ctrl+T`) provides eight handles, a rotation handle, scale,
  rotate, skew, distort and perspective controls. Shift keeps scale proportions
  and snaps rotation to 15 degrees. Enter applies; Escape cancels. Every preview
  uses an immutable source and Apply performs one full-resolution CPU resample.
  A +10/-10 degree uncommitted session preserves exact original samples and adds
  no history entry. Separate committed destructive transforms each resample.
- Nearest, bilinear, bicubic and Lanczos3 are available; new sessions default to
  Lanczos3. Axis-aligned resizing uses fast_image_resize 6.1.0's float API;
  arbitrary matrices use the Rust sampler. Identity/integer permutations preserve
  complete straight-alpha samples, including hidden RGB. Selection transforms
  lift and clear coverage once, resample once and composite once. Whole-layer
  transforms include linked masks; selected-content transforms keep the mask fixed.
- Image size/DPI, anchored canvas size, whole-canvas arbitrary/right-angle rotation
  and flips include nested layers, masks, selections and guides in exact commands.
  Bounds are scanned in pageable chunks. Sparse movement does not accumulate
  empty tile padding, and retained rebasing preserves the visible screen anchor.
- Crop (`C`) offers drag/resize handles, Alt-drag movement, ratio presets,
  rule-of-thirds and a drawn/numeric straighten angle. Ordinary crop changes only
  the visible window. Straighten rotates retained content once and crops in one
  command. Reveal All restores hidden content in its transformed coordinates;
  undo restores the original samples. Outside raster/mask samples persist through
  streamed project/recovery files and paging. Crop-edge mips exclude hidden pixels.
- Rulers, editable document guides and snapping use document coordinates through
  zoom/rotation/flip. Streamed schema 4 stores crop/guide metadata, with schema
  2/3 reading. Export visits the visible canvas. All document mutations use commands.

Pixel processing stays in Rust; TypeScript handles geometry and controls, never
document pixel loops. The worker owns document state, sources, jobs and GPU caches.
Candidates hydrate/yield/persist in bounded steps and publish atomically. Invalid
matrices, locks, cancellation, storage failure and budget refusal preserve document,
history and redo. Worker replacement clears abandoned geometry control ownership.
Budgets remain 768 MiB CPU / 128 MiB GPU, with retained/source/candidate/scratch/cache
accounting, 16,384-pixel sides, 128 MiB images and 4 GiB streamed projects.

## Numerical and visual evidence

The independent image 0.25.10 Lanczos3 reference downscales the documented
1030x518 opaque fixture to 50%, with maximum RGBA error `[1, 0, 1, 0]` against
the allowed two. Separate tests cover fractional alpha/hidden RGB, transparent
edges, masks, odd tile/block boundaries, selection coverage, cold paging,
permutations, refusal/cancellation and exact undo/redo. The native reference is
`resample_reference.rs`; raw output is `bench/results/p7-lanczos-reference.log`.

GPU preview is approximate: immutable sources are cached at up to 1536 pixels on
their longest retained side, and filter support is capped at four cache pixels
per axis. Apply uses the full-resolution source and full reduction support. A
masked 10-degree rotation matches committed output within one channel value at a
pixel-aligned view, after rotated/flipped cache rebuild. Rotated normal-viewport
atlas interpolation and preview pixel-center sampling can choose neighboring
pixels near integer boundaries; the recorded small fixture had three pixels with
error above one, maximum three. No universal rotated-screen pixel parity is claimed.
The transparency checkerboard now stays anchored to the visible canvas origin.

The crop draft displays the straighten line and angle; image rotation happens
on Apply. Ordinary crop retains original generations. Straighten and other
destructive transforms retain outside content after resampling; the history
command supplies exact restoration of the original orientation and samples.

## Verification

The implementation checkpoints pass all 153 Rust tests, native and WASM clippy,
Rust/JS formatting, TypeScript, ESLint, 16 Vitest cases and all 451 approved
dependency licenses. The production WASM is 3,820,283 bytes; gzip level 9 is
1,423,577 bytes, under the 1,500,000-byte gate. No dependency was added after the
approved fast_image_resize increment.

Installed Chrome/Edge tests cover both hardware APIs. The Windows Tauri shell
builds; WebView2 155.0.4283.62 reports NVIDIA for WebGPU and NVIDIA GeForce GTX
1650 / D3D11 for WebGL2. Its expanded smoke passes transform inverse/immutability,
single-command movement, straightening, exact undo, streamed crop reopen, Reveal
All and the earlier phase checks. Software results are functional evidence only.

The complete 440-case hardware run passed in 21.7 minutes, each case on one
attempt, with zero errors/skips/retries. The final focused crop run passes all
20 cases, including four additional regressions proving an exactly aligned edge
wins over a nearby guide during perpendicular movement. This covers 444 distinct
hardware cases. Three repeated software checks verify the older selection test
now waits for its frame's exact visited-pixel count. The Windows smoke was rebuilt
and rerun against the final crop interaction code on both hardware APIs.

The corrected sampler additionally passes all 116 applicable hardware geometry/
viewport cases. Final UI verification exercises the global Cancel operation
button for generic geometry jobs, preserving exact samples and redo, along with
the transform controls and worker-loss recovery; all 32 cases pass on one attempt
each across both installed browsers/APIs. This control is needed for the
slow full-resolution Apply workloads measured below.

The processing checkpoint `e188820` has green
[CI](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40374982974)
and [deployment](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40374982935).
Its inspected artifact contains 329 software browser cases on one attempt each,
zero errors/skips/retries, and both software WebView2 backends passing. CI WASM
is 3,821,275 bytes / gzip9 1,423,641. These software measurements are functional
checks only. The final UI/report commit is also gated by remote CI/deployment;
the release tag's annotation identifies that final run and its inspected artifact.

Diagnostic evidence is retained for the initial snapshot race, overlapping DOM
handles in screenshot comparisons, ambiguous duplicate control locator, checker
origin shift, rotated atlas sampling precision and the unreliable old after-step
budget fault. The latter now refuses an allocation immediately after a real step;
production budgets and atomicity assertions are unchanged. The superseded full
run was stopped; it is not a passing result. The older `5e3dd87` CI had a retry
and `929cbc3` CI failed a filter-preview screenshot. That test now waits for the
committed content revision and inactive preview before its two animation frames,
preserving exact equality. Those older runs do not satisfy phase acceptance.

The `cb46084` CI reached the old 40-minute job envelope after 34.5 minutes of
software tests. Its partial artifact also contained masked-preview equality
failures and one stale selection-frame read that passed on retry, followed by
unexecuted/interrupted cases. It is not accepted. The pixel comparison now checks
actual rotated-layer resampling within one channel value at an aligned camera,
and the selection metadata assertion waits for its frame. The CI job envelope is
60 minutes; individual tests and all required backends remain mandatory.

## Hardware measurement method

`pnpm bench:geometry` measures the resident 6000x4000 NASA/JPL-Caltech/IPGP photo
already used by P5/P6. The report records the fixture and WASM SHA-256, browser,
adapter, CPU, OS and viewport. Four Apply scenarios cover 50% Lanczos3 image
resize, exact whole-layer movement, a 10-degree Lanczos3 whole-layer rotation and
movement of a 3000x2000 selection. Each has one first application and three warm
repeats after restoring the original by undo. Warm nearest-rank p50/p95/p99 use
only three observations, so p95/p99 are maxima, not well-estimated tail latencies.

Preview measurement separates session/source preparation from 20 serial warm
updates per interpolation mode. API return/presentation acknowledgements include
worker scheduling and GPU completion; pointer event coalescing, compositor/scanout
and physical input latency are outside this measurement. CPU/GPU numbers are
logical resident buffer accounting, excluding allocator/driver overhead. Apply
totals include required persistence/hydration/compositing; this is not a disk-cold
opening benchmark. Hardware cases run sequentially without concurrent GPU tests.
P7 specifies no separate millisecond target, so none is invented here.

An initial 24 MP integer move took 65-76 seconds because cofactor inversion
changed exact unit coefficients by floating-point roundoff, missing the tile-copy
path. The final sampler recognizes exact permutations from the forward matrix
and constructs their inverse exactly. A regression verifies one tile per step,
bounded dependencies and exact translated hidden RGB under homogeneous matrix
scaling. The superseded measurements are retained under
`p7-geometry-initial-slow-permutation.*`; final timings below use the corrected build.

Measured on Windows 10 Pro, Intel(R) Xeon(R) CPU E5-2680 v3 @ 2.50GHz, 32688.7 MiB RAM, NVIDIA GeForce GTX 1650. Browser versions: chrome: 156.0.8078.47; msedge: 156.0.4314.42. Viewport: 1280x850, DPR 1; fitted preview output: 878x494. Source cache longest side: 1536 pixels.

Fixture SHA-256: `603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`. Measured WASM SHA-256: `5aae18218f6b0af2bd7bda8c9986a2714b05968130b88e2aa5f52d7da956b3ec`.

All 64 Apply trials and 320 warm preview updates completed on actual hardware. Reported memory is logical residency; Apply CPU peaks combine worker-step and frame-sampled maxima. Preview memory below is sampled warm residency, not process memory or a measured cold-preparation peak. First means the first call in the worker after image opening; driver caches and background OS activity were not controlled. Admission waits, including an overlapping autosave, remain part of API totals. No concurrent GPU tests ran during measurements.

| Browser / API   | Apply                 | First (s) | Warm p50 / p95 / p99 (s)    | CPU peak (MiB) | GPU peak (MiB) |
| --------------- | --------------------- | --------: | --------------------------- | -------------: | -------------: |
| Chrome / WebGPU | 50% Lanczos3 resize   |     4.001 | 2.820 / 2.882 / 2.882       |          160.3 |           16.1 |
| Chrome / WebGPU | Exact layer move      |     3.422 | 3.257 / 3.925 / 3.925       |          304.3 |           16.1 |
| Chrome / WebGPU | 10° Lanczos3 rotation |   175.245 | 177.475 / 178.065 / 178.065 |          318.0 |           16.1 |
| Chrome / WebGPU | 6 MP selection move   |     6.209 | 6.250 / 6.295 / 6.295       |          254.0 |           16.9 |
| Chrome / WebGL2 | 50% Lanczos3 resize   |     3.963 | 2.752 / 2.924 / 2.924       |          160.3 |           16.1 |
| Chrome / WebGL2 | Exact layer move      |     3.225 | 3.251 / 3.492 / 3.492       |          304.3 |           16.1 |
| Chrome / WebGL2 | 10° Lanczos3 rotation |   157.038 | 161.114 / 161.205 / 161.205 |          318.0 |           16.1 |
| Chrome / WebGL2 | 6 MP selection move   |     6.122 | 6.198 / 6.332 / 6.332       |          254.0 |           16.9 |
| Edge / WebGPU   | 50% Lanczos3 resize   |     4.053 | 2.863 / 2.926 / 2.926       |          160.3 |           16.1 |
| Edge / WebGPU   | Exact layer move      |     3.348 | 3.191 / 3.638 / 3.638       |          304.3 |           16.1 |
| Edge / WebGPU   | 10° Lanczos3 rotation |   163.993 | 164.380 / 164.815 / 164.815 |          318.0 |           16.1 |
| Edge / WebGPU   | 6 MP selection move   |     6.390 | 6.582 / 6.585 / 6.585       |          254.0 |           16.9 |
| Edge / WebGL2   | 50% Lanczos3 resize   |     3.988 | 2.772 / 2.780 / 2.780       |          160.3 |           16.1 |
| Edge / WebGL2   | Exact layer move      |     3.259 | 3.182 / 3.581 / 3.581       |          304.3 |           16.1 |
| Edge / WebGL2   | 10° Lanczos3 rotation |   156.311 | 160.265 / 164.804 / 164.804 |          318.0 |           16.1 |
| Edge / WebGL2   | 6 MP selection move   |     5.997 | 6.193 / 6.369 / 6.369       |          254.0 |           16.9 |

| Browser / API   | Preview interpolation | Warm API p50 / p95 / p99 (ms) |
| --------------- | --------------------- | ----------------------------- |
| Chrome / WebGPU | nearest               | 19.95 / 39.17 / 59.36         |
| Chrome / WebGPU | bilinear              | 17.00 / 23.01 / 23.01         |
| Chrome / WebGPU | bicubic               | 16.22 / 19.19 / 19.78         |
| Chrome / WebGPU | lanczos3              | 17.02 / 25.01 / 25.55         |
| Chrome / WebGL2 | nearest               | 21.46 / 26.38 / 28.17         |
| Chrome / WebGL2 | bilinear              | 16.98 / 18.14 / 30.29         |
| Chrome / WebGL2 | bicubic               | 16.89 / 18.80 / 30.19         |
| Chrome / WebGL2 | lanczos3              | 16.93 / 18.08 / 30.36         |
| Edge / WebGPU   | nearest               | 20.76 / 31.42 / 85.66         |
| Edge / WebGPU   | bilinear              | 16.77 / 25.47 / 26.78         |
| Edge / WebGPU   | bicubic               | 16.83 / 20.96 / 22.15         |
| Edge / WebGPU   | lanczos3              | 16.42 / 18.19 / 18.24         |
| Edge / WebGL2   | nearest               | 22.95 / 31.41 / 31.84         |
| Edge / WebGL2   | bilinear              | 16.45 / 19.55 / 20.00         |
| Edge / WebGL2   | bicubic               | 16.56 / 28.97 / 32.75         |
| Edge / WebGL2   | lanczos3              | 16.81 / 18.17 / 31.21         |

| Browser / API   | Begin (ms) | First preparation (ms) | First update (ms) | Warm preview CPU / GPU peak (MiB) |
| --------------- | ---------: | ---------------------: | ----------------: | --------------------------------- |
| Chrome / WebGPU |     113.55 |                1561.64 |            214.41 | 209.7 / 35.5                      |
| Chrome / WebGL2 |     143.09 |                1905.84 |           1138.53 | 209.6 / 35.5                      |
| Edge / WebGPU   |     114.03 |                1606.81 |            273.63 | 209.6 / 35.5                      |
| Edge / WebGL2   |     116.70 |                1803.79 |           1163.89 | 209.6 / 35.5                      |

Full-resolution arbitrary Lanczos3 rotation is slow on this single-threaded implementation: approximately 2.6-3 minutes for this photo. Fast cached previews do not imply fast CPU Apply. Geometry jobs remain bounded and cancellable, and the global Cancel operation control covers generic geometry work. CPU optimization remains future work; this report does not waive a specified timing requirement or invent one.

Raw trials, kernel/worker/presentation timing splits, category maxima and adapter identity are in `bench/results/p7-geometry-performance.json`.

## Deferred work and phase boundary

Warp and puppet warp are deferred until after v1; content-aware scale remains
outside v1, as selected by the user. P5's identifiable Photoshop feather comparison
remains pending without `p5-select`; physical Windows Ink retains its approved
deferral. P6-M4-REPORT.md's legacy WebGL2 brush/frame-latency and first-use preview
limits remain separate from P7 functional acceptance.

Proposed next phase: P8 extended formats/export and validated PSD interoperability.
Do not start it, select new format dependencies or claim Photoshop compatibility
until the user starts P8 and its product/fixture questions are resolved.
