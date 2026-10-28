# P9 M2 - Healing Brush and Spot Healing

Implementation, hardware measurements and local verification are complete.
Final acceptance requires exact-final-head remote CI/deployment with inspected
artifacts. The milestone completion message and
`bench/results/p9-m2-final-certification.json` record the immutable final evidence.
This milestone does not complete P9; no `p9-retouch` tag is authorized yet.

Started 2026-10-28 directly on clean main at certified M1
`c630690977e1a3ca3a5558f2dd0d3ee78808540f`, matching fetched origin/main, without
worktrees or agents. Read all phase records, M1 report/certification and the
complete original P9 section on printed page 17 as text and a rendered page.
The original PDF SHA-256 matches
`83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.
Existing product decisions answer every M2 question. P9-DESIGN.md records M2
behavior and acceptance before implementation. Stop before M3 content-aware fill.

## Delivered behavior

- Healing Brush / J: Alt-click an immutable source, aligned/non-aligned sampling,
  current raster or all visible layers. Source picking and its marker reuse M1;
  the accessible marker identifies the Healing source. New/open/recovery resets
  the anchor. Foreground color is irrelevant.
- Spot Healing / Shift+J: paint a blemish without choosing a source. An original
  deterministic nearby patch search excludes overlapping source patches and
  scores an outside ring, avoiding the blemish interior. Coarse candidates use
  three distances and sixteen angles, followed by bounded local refinement.
  Only the next candidate's source/target tiles are hydrated.
- An original Rust conjugate-gradient solver preserves source gradients while
  matching immutable destination Dirichlet boundary samples on each dab's shaped
  domain. It solves RGB together with f64 vectors, verifies the true residual
  before publishing a solved dab, and refuses non-convergence. The numerical
  reference is an independent dense direct equation solve; no GPL code or new
  processing dependency is used.
- Existing tips/dynamics, hardness, flow, opacity, spacing and stabilizers apply.
  Selection clips writes exactly once. All-visible sampling supports an empty
  retouch raster; the visible immutable target supplies its boundary. Source-over
  alpha behavior and alpha lock remain. Zero-alpha RGB never guides a solve.
  Mask/Quick Mask targets are explicitly refused, as in Clone Stamp.
- One named Healing Brush / Spot Healing command per stroke; exact undo/redo.
  Undo/Redo arriving during worker work wait in the existing bounded interaction
  queue instead of being discarded during a compositing/autosave race.
  Overlapping dabs and nearby search never sample preview pixels. Source, target
  and halo respect the visible crop. Retained samples, unrelated hidden RGB,
  masks/selections, guides/DPI and legacy project/recovery readers retain their
  contracts.
- Escape, focus loss, pointer cancellation, overflow, failed hydration and memory
  refusal discard the candidate without changing committed pixels/history/redo.
  Escape/focus loss also cancel after pointer release while CPU work remains;
  normal lost-pointer-capture on release lets that pending work finish.

Pixel processing stays in the independent Rust processing crates. The engine
worker owns document/history/GPU/source/search/solver state; React/zustand owns
controls. No M3-M5 tool or P10 work is included. Runtime dependencies are unchanged;
the generated inventory still approves 459 license expressions. The public-domain
reference photograph's source, photographer and hash are recorded in
`tests/fixtures/retouch/README.md` and docs/LICENSES.md.

## Resource and algorithm bounds

The solver processes at most 8192 vector elements per stage chunk; a stroke step
has a bounded work allowance and returns to the worker event loop. Search uses
48 ring probes per candidate and at most 173 candidates for supported brush sizes.
The complete dense patch allocation is admitted before allocation: four f64 RGB
vectors, two RGBA8 surfaces and a byte domain total 105 bytes per halo pixel,
plus 1536 bytes of candidate storage. Coverage/COW output tiles, the bounded input
queue, procedural tip, immutable source generations, document/history, paging,
display/composition and GPU residency remain in the shared model.

Limits remain 768 MiB logical CPU / 128 MiB GPU, 16,384-pixel retained/visible
sides, 128 MiB encoded images, 4 GiB streamed projects and 2,000,000 gzip WASM
bytes. Large jobs can be refused within these budgets. Logical capacities exclude
allocator/map/Arc retention and browser/driver overhead; they are not physical
RAM/VRAM measurements. Search/solver state is released on commit/cancel/refusal.
Completed stationary dabs reuse their immutable solved patch when position and
radius match exactly. Numerical/search scratch is released first; retained samples
and domain cost five bytes per halo pixel and remain accounted. Moving or changing
radius invalidates this single-entry stroke-local cache before a new allocation.

The algorithm is local per dab, not a general object remover. A blemish must fit
inside the brush boundary, and Spot Healing needs suitable nearby visible texture.
Edges, transparent holes and unsuitable neighborhoods can constrain the result or
cause an atomic refusal. Larger brushes and queued strokes take more CPU time;
cooperative yielding provides cancellation rather than a frame-rate healing claim.

## Identifiable portrait seam evidence

The unchanged reference is **Defense.gov News Photo 071212-F-6244S-034**, showing
Staff Sgt. Melissa Diaz, photographed by Staff Sgt. Eric T. Sheler, U.S. Air Force,
on 12 December 2007. It is a public-domain federal government photograph:
https://commons.wikimedia.org/wiki/File:Defense.gov_News_Photo_071212-F-6244S-034.jpg.
The 1905x2889 JPEG is 1,499,675 bytes, SHA-256
`aac5806caab9b8e3d897846ee5dcef29890f14581492a210517439e240fbb014`.
The fixture contains actual blemishes; none is injected.

Both tools treat the cheek blemish at document `[1466,1223]`. Healing source is
`[1420,1180]`; Spot Healing chooses automatically. Hard round size 48, hardness,
opacity and flow one, current-layer sampling, mouse pressure one, no selection.
Full before/after photographs, ordinary app captures at 100% and 2x neighborhood
crops were inspected. The blemish is removed with no visible circular boundary,
tonal seam or tile edge. Other blemishes and all samples outside the dab support
remain untouched. This is a result for this specified real fixture/settings,
not a claim about every image or a synthetic-only comparison.

The center changes from `[228,101,86,255]` to `[242,151,133,255]` (Healing Brush)
or `[233,142,125,255]` (Spot Healing). The RGB contrast check is supplemental;
visual inspection establishes the seam assessment. All samples outside
`[1442,1199,1491,1248)` compare byte-exact. Chrome/Edge and both APIs produce
identical before/after PNG hashes per tool:

- Before: `7dd38840a176635bcb31144398e943d231c85e15ce90f3393f13f0c897cac876`.
- Healing: `3a534dfb611e19803a8f2bf9f9a1a9a8c41b4bb449d4fd6d635452933c53385a`.
- Spot: `9e2cdd304fd5b8231eb3bcd17b94e17bdbbe9819801a6e09c61632b96c20f21d`.

Evidence: `bench/results/p9-m2-portrait-*`, including full before/after PNGs,
UI captures, `portrait-review.png` and `portrait-evidence.json`. Reproduce via
the portrait cases in `tests/e2e/healing.spec.ts`. These altered QA outputs are
identified separately from the unchanged source photograph.

## Hardware measurements

All 32 hardware configurations complete: 128 click strokes and sixteen short
paths. Warm 48 px click strokes take 74.43-135.14 ms; 200 px clicks take
3.25927-3.90813 seconds. Large healing brushes remain slow on this CPU. The
original roadmap has no separate M2 fps/ms target; no new exception or frame-rate
healing claim is inferred. Software rendering establishes function only.

Windows 10 Pro 10.0.19045; Xeon E5-2680 v3 at 2.50 GHz;
34,276,601,856 bytes RAM; NVIDIA GTX 1650/Turing, driver 27.21.14.5751. Chrome 156.0.8078.49 and
Edge 156.0.4314.42, serial non-fallback NVIDIA WebGPU and NVIDIA ANGLE/D3D11
WebGL2, 1280x850 / DPR 1. No Codex builds/tests run during timing. This is a
shared machine, not a dedicated-host claim.

The ordinary dense 6000x4000 RGBA8 fixture is 67,038,649 bytes, SHA-256
`809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5`.
Normal populated document view at 100%, no diagnostic texture. Current mode
edits the image raster; all mode edits an empty retouch raster above it. Hard
round, hardness/opacity/flow one, pressure one, no selection/masks/crop. Healing
source offset `[0,-180]`, aligned; Spot Healing searches automatically. Functional
cases separately cover clipping, masks, locks and metadata.

Each browser/API/source mode uses a fresh context. Four native click strokes per
tool/size include down/up endpoint samples, with exact undo outside timing to
restore the source. First means the first call with these controls in that
context; shared kernels can already be warm from preceding configurations. Three
warm calls follow. OS/file caches are not cleared; no disk-cold claim. Times run
from native click request to observed committed presentation, including automation,
worker queuing, CPU processing, uploads, GPU acknowledgement and persistence.

All values below are milliseconds: **first / warm p50 / p95 / p99**. With only
three warm calls, empirical p95/p99 are the maximum observed call, not a claim
about long-term tail confidence.

| Browser / API   | Source  | Size | Healing Brush                         | Spot Healing                          |
| --------------- | ------- | ---: | ------------------------------------- | ------------------------------------- |
| Chrome / WebGPU | current |   48 | 74.71 / 75.91 / 77.45 / 77.45         | 91.40 / 77.33 / 77.81 / 77.81         |
| Chrome / WebGPU | current |  200 | 3665.69 / 3712.37 / 3748.47 / 3748.47 | 3247.33 / 3308.36 / 3309.65 / 3309.65 |
| Chrome / WebGPU | all     |   48 | 110.49 / 91.41 / 96.39 / 96.39        | 102.75 / 94.35 / 120.62 / 120.62      |
| Chrome / WebGPU | all     |  200 | 3842.26 / 3856.59 / 3873.41 / 3873.41 | 3472.60 / 3459.92 / 3491.87 / 3491.87 |
| Chrome / WebGL2 | current |   48 | 92.11 / 85.67 / 91.74 / 91.74         | 78.55 / 76.01 / 76.97 / 76.97         |
| Chrome / WebGL2 | current |  200 | 3837.34 / 3825.92 / 3828.56 / 3828.56 | 3450.67 / 3440.51 / 3480.31 / 3480.31 |
| Chrome / WebGL2 | all     |   48 | 144.81 / 92.05 / 121.67 / 121.67      | 92.93 / 102.69 / 103.81 / 103.81      |
| Chrome / WebGL2 | all     |  200 | 3917.90 / 3896.14 / 3908.13 / 3908.13 | 3521.20 / 3545.99 / 3559.22 / 3559.22 |
| Edge / WebGPU   | current |   48 | 93.45 / 89.64 / 90.95 / 90.95         | 73.88 / 76.33 / 76.92 / 76.92         |
| Edge / WebGPU   | current |  200 | 3714.77 / 3613.62 / 3680.45 / 3680.45 | 3286.07 / 3399.14 / 3430.97 / 3430.97 |
| Edge / WebGPU   | all     |   48 | 110.43 / 88.78 / 89.46 / 89.46        | 106.08 / 92.64 / 106.25 / 106.25      |
| Edge / WebGPU   | all     |  200 | 3877.53 / 3710.66 / 3777.65 / 3777.65 | 3337.76 / 3488.15 / 3491.17 / 3491.17 |
| Edge / WebGL2   | current |   48 | 92.39 / 76.72 / 76.99 / 76.99         | 76.60 / 76.80 / 89.15 / 89.15         |
| Edge / WebGL2   | current |  200 | 3832.98 / 3840.86 / 3852.90 / 3852.90 | 3452.54 / 3427.62 / 3442.80 / 3442.80 |
| Edge / WebGL2   | all     |   48 | 95.03 / 91.03 / 98.95 / 98.95         | 102.78 / 104.07 / 135.14 / 135.14     |
| Edge / WebGL2   | all     |  200 | 3925.84 / 3857.47 / 3886.06 / 3886.06 | 3550.76 / 3525.34 / 3526.15 / 3526.15 |

Each 48 px configuration also draws a 48 px horizontal path: down, four native
12 px moves with requested 16 ms pauses, up. Actual delivery includes protocol
overhead; no fixed input Hz is claimed. These paths finish in 660.32-824.46 ms
(current) and 776.90-886.02 ms (all), with at most six pending samples. Pooled
changed-sample-to-GPU-ack p50/p95/p99 are 251.06/609.09/642.92 ms (current) and
275.05/682.42/713.41 ms (all). Queuing is included; this is not under-one-frame
healing. No 200 px path throughput claim is made.

Click-sample-to-changed-GPU-ack spans 57.41-80.46 ms at 48 px current,
67.77-135.19 ms at 48 px all, 3.21090-3.80984 s at 200 px current and
3.23419-3.82906 s at 200 px all. Completion acknowledgements can include unchanged
endpoint dabs. They exclude compositor/scanout and physical input latency.
Active frame cadence includes unchanged previews: 200 px p50 ranges
16.48-17.33 ms, but changed pixels arrive only after the solve. Frame cadence
must not be reported as sixty changed healing updates per second.

Observed logical CPU peaks are 102.26 MiB (current) and 198.26 MiB (all), GPU
32.11 / 20.38 MiB. The synchronous flat compositor's admitted reference/output
allowance can add at most 2.25 MiB beyond an observed snapshot. These remain
below 768 / 128 MiB; no physical RAM/VRAM claim is made.

The repeated 200 px Clone Stamp hardware matrix measures current presentation
59.63-60.12 fps and changed acknowledgements 44.65-52.60/s; all-visible sampling
26.66-30.87 fps and 22.67-26.40 changed acknowledgements/s. The all-layer original
60 fps target still fails and retains its existing user-approved P12 deferral.
Unchanged preview frames do not establish sixty distinct pixel updates/s. The
per-API cadence/latency distributions and first-use conditions are retained in
the clone regression files; no new M2 speed waiver is inferred.

Raw matrices, runtime asset SHA-256s and ordinary-view screenshots are
`bench/results/p9-m2-performance.json`, `p9-m2-performance-summary.json` and
`p9-m2-clone-{current,all}-regression-performance.json`. Reproduce with
`pnpm bench:healing` and `pnpm bench:clone` (`CLONE_ALL=1` for all-visible clone).
Measured runtime is `0004c959be67f35aefaa72d7ab6cb0ea06d8a63e`; later report changes
do not alter its processing assets.
The 32-capture `p9-m2-benchmark-contact.png` was inspected to confirm ordinary
populated document presentation throughout the matrix.

## Verification

The 196 Rust tests include independent direct Poisson equations, source-gradient
versus destination-tone adaptation, deterministic non-overlapping spot search,
immutable overlapping paths, alpha/flow/opacity/fractional selection, tile/crop
boundaries, hidden RGB, locks, memory refusal, cold halo requests, no-op redo and
exact command undo/redo. Native/WASM clippy, formatting, TypeScript, ESLint,
16 Vitest tests and the 459-expression license inventory pass. Production WASM
is 4,475,194 bytes / gzip9 1,653,033, within the unchanged 2,000,000-byte gate.

All 48 final focused installed Chrome/Edge WebGPU/WebGL2 cases pass once with
zero skips/retries/flaky cases/global errors, including M1 Clone Stamp regression,
actual source picking, separate-layer healing, selection, visible cancellation,
post-release Escape/focus-loss cancellation, queued busy-worker Undo/Redo and the
real portrait. Rebuilt
WebView2 passes both NVIDIA hardware APIs with both healing tools' pixel,
one-command and exact undo/redo checks plus the existing broad desktop smoke.
Evidence: `p9-m2-queued-focused.*`, `p9-m2-queued-desktop-*`.
The complete 612-case installed Chrome/Edge WebGPU/WebGL2 hardware regression
passes in 29.0 minutes, one attempt each, with zero skips, retries, flaky cases or
global errors. Its complete case/result records were inspected, not only the
console summary. Evidence: `p9-m2-final-browser.json` and its verification summary.
All 36 focused local software cases also pass once without skips, retries, flaky
cases or global errors, including held-worker history serialization. Evidence:
`p9-m2-software-focused.json` and its verification summary. This is functional
evidence, not hardware performance.
Exact-head remote certification remains mandatory. The deployment workflow retains
its exact Linux static bundle so every hosted runtime file can be compared with
the downloaded final-head artifact before acceptance.

Earlier investigations are retained separately. The first portrait harness
serialized PNG bytes as millions of JSON numbers, overloading protocol tracing;
base64 transfer fixes the test harness. An interrupted browser launch and a local
preview 404 while assets were rebuilt are not acceptance evidence. Build and
browser execution are now sequenced against fixed assets. The post-release
cancellation review closed a longer-job gap in the existing pointer foundation;
normal release still completes a job. Final acceptance never substitutes an
earlier green/retried/interrupted report for clean exact-head evidence.
The first complete timing attempt stopped when an Undo restore raced worker work.
Its partial rows are retained separately; the worker now queues history input,
and a held-job test proves exactly-once Undo/Redo without retrying clicks. Final
timings use the corrected worker and retain explicit failure snapshot guards.

## Deferred work and next milestone

M3 is the proposed next milestone: own PatchMatch content-aware fill, with bounded
cancellable candidate processing and exact command undo. It has not started.
M4/M5 and final P9 acceptance still gate `p9-retouch`; P10 remains outside scope.

M1's accepted all-layer 60 fps miss remains deferred to P12 and is not declared
passed. P5's identifiable Photoshop feather comparison remains pending without
`p5-select`. Physical Windows Ink retains its user-approved deferral. Recorded
P6/P7 limits and P8's accepted save-speed deferral remain unchanged. No new M2
exception is requested or inferred.
