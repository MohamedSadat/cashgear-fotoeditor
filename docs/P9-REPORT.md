# P9 - Retouch and advanced painting

P9 delivers Clone Stamp, healing/spot healing, content-aware fill, six retouch
brushes, editable gradients and advanced painting. M5 adds image tips, seeded
scatter/jitter, texture, CashGear brush libraries and mirror/radial painting.
Phase completion requires the remaining gates below and the immutable final
certificate before creating `p9-retouch`. P10 has not started.

**Current-layer clone performance is being revalidated:** the pre-optimization
phase matrix measures 56.65-57.84 presentation fps, including WebGL2 misses. The
existing user-approved P12 exception covers all-visible sampling only. No new
exception is inferred, and the original current-layer 60 fps target is not
declared passed. On 2026-11-04 the user retained 60 fps and requested continued
optimization; no additional deferral is authorized.

The subsequent complete candidate matrix measures 59.84-60.01 presentation fps,
within M1's previously accepted nominal 60 Hz range. Opaque-row specialization,
one shared document/history residency walk, contiguous display conversion and
2D cursor transforms preserve exact samples and budgets. This remains a candidate
until clean-commit measurements and all final verification pass. The original
whole-window fps metric is retained alongside actual GPU-completion traces.

## Baseline and delivered scope

M5 started on 2026-11-03 directly on clean main matching origin/main at accepted
M4 `7d4f28313f83ad66faab5b67df1e515c4e7d7050`, without worktrees or agents. Read
AGENTS.md, PLAN, PROGRESS, DECISIONS, P9-DESIGN and the M4 report. Re-read the
complete original roadmap on printed page 17 as text and a newly rendered page.
Its SHA-256 remains
`83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.
M5 behavior/acceptance was recorded before implementation. Existing product
choices answered the phase questions.

| Milestone | Delivered behavior                                                                                                                              |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| M1        | Immutable aligned/non-aligned Clone Stamp, current/all-visible sampling, source picking/indicator and exact strokes.                            |
| M2        | Original bounded Poisson healing and nearby patch-search Spot Healing; licensed real-portrait evidence.                                         |
| M3        | Original multiscale PatchMatch content-aware fill, immutable donors excluding selection, cooperative cancellation/admission and exact commands. |
| M4        | Smudge, Blur, Sharpen, Dodge, Burn and Sponge; five gradient shapes, a 2..32-stop RGBA editor and reusable gradients.                           |
| M5        | Image coverage tips, scatter, size/angle/opacity jitter, texture, CashGear preset import/export and mirror/radial painting.                     |

Detailed earlier behavior, numerical references and quality limits remain in
P9-M1-REPORT.md through P9-M4-REPORT.md. Their accepted evidence is preserved.
Both immutable M3/M4 certificate hashes match the handoff. An independent hash
verification checks every one of their 520 listed raw artifacts, including
their reports and attempt classifications, without a mismatch. New regression
portraits use M5 prefixes.

## M5 controls and processing

Open **Image tips, dynamics & presets** in Brush & Color. Load an 8-bit PNG,
JPEG or WebP up to 512x512 pixels and 4 MiB. Rust checks dimensions/decoder
capacity before decoding, then converts inverse Rec.709-weighted sRGB channel
luminance times alpha to coverage. Black paints; white/transparent samples do
nothing. Hidden RGB cannot contribute. These are coverage assets, with source
channels interpreted as sRGB; they do not replace the document image importer.

Image tips retain aspect ratio, bilinear sampling, roundness, angle and existing
pen dynamics. Size is the circumscribed diameter. The procedural round remains
available; pencil keeps its exact document-pixel behavior. Image coverage
supplies the tip shape instead of procedural hardness. Very small/tiny-aspect
tips can lose detail during resampling; this is not a higher-resolution stamp
or texture-synthesis quality guarantee.

Scatter offsets a dab in a disk with radius 0..2 brush diameters. Size jitter
reduces its size, opacity jitter reduces influence, and angle jitter spans up
to +/-180 degrees. A user-editable u32 seed initializes a deterministic Rust
sequence once per stroke. Identical input samples/settings repeat the result,
independent of cooperative slice size or hydration order. Input timing still
matters when a time-dependent stabilizer or stationary spray is selected.

Texture is a repeating coverage image in the dab's local coordinates, with
1..512-document-pixel period and 0..1 depth. Coverage is multiplied before
flow/selection. Image/jitter/texture settings work through all stroke pipelines,
including erasing, clone, healing and retouch. Each retains its existing sampling,
alpha, strength and stroke-opacity contract.

Painting symmetry offers vertical, horizontal and both mirrors, plus 2..16 radial
sectors. Edit the center in retained document coordinates or center it on the
visible canvas. Axes/spokes follow pan, zoom, rotation, flip and crop. Rust
replicates each jittered dab and its oriented image. At coincident centers,
distinct orientations form a maximum-coverage union before flow applies once;
identical round coverage is deduplicated. The bounded orientation arrays are
included in admission. Brush/Eraser include layer masks and Quick Mask. Retouch tools
explicitly use a single path, preserving their source-mapping behavior.

Named presets save brush shape/dynamics, tip/texture, scatter/jitter and seed.
Foreground color, active tool and document-relative symmetry stay separate.
Save/update by name, delete and reuse across documents/restarts. Export/import
self-contained version-1 `.cgbrush` JSON libraries: up to 32 unique names of
at most 64 characters, 4 MiB encoded UTF-8 and canonical bounded coverage assets.
Photoshop ABR is outside the selected scope. An entire import is validated and
stored before replacing the live preset library. Corruption, future versions,
oversized input or storage failure leave presets/settings/document/history/redo
unchanged and show an error. Projects/recovery keep their existing schemas.

## Commands and residency

Every document mutation uses the existing command path. A replicated stroke
is one command with exact undo/redo and sparse absence. Selection clips once;
alpha lock and hidden RGB, masks, retained crop pixels, guides/DPI, paging,
recovery and legacy project reading keep their contracts. Escape/focus/capture
loss, queue overflow, missing pages and admission failure discard the whole
candidate without clearing redo or publishing a partial edit.

The five processing crates remain independent of browser/UI/GPU types. Pixel
work stays in Rust; the worker owns document/candidate/GPU state and React/zustand
owns controls/preferences. No dependency is added. The approved inventory stays
at 459 license expressions; every original budget and file cap remains unchanged:
768 MiB logical CPU, 128 MiB GPU, 16,384 maximum sides, 128 MiB document images,
4 GiB projects and 2,000,000 gzip WASM bytes.

Admission includes canonical decoded settings/assets, prepared tip/texture,
the 2,048-sample Rust input queue, at most 16 planned symmetry dabs, coverage/COW
output/source/history, paging and display scratch. Host and worker additionally
cap queued brush payloads at 16 MiB by string capacity plus settings overhead;
the worker reserves retained queued payloads alongside its external buffers.
Reservations are released on consumption/cancellation. Unchanged reservations
are reused between frames. Native work remains bounded by 262,144 destination
positions and 512 new dab attempts per step. These are work bounds, not deadlines.

Logical capacities exclude allocator/map/Arc metadata, retained WASM heap pages,
UI/browser/driver overhead and physical process/VRAM measurements. Snapshot
peaks do not exhaustively observe every intra-step allocation; conservative
admission and retained-generation accounting enforce the processing budgets.

## Hardware measurements

The pre-optimization M5 matrix has 16 configurations and 64 successful strokes. Every
configuration restores its exported before/after SHA-256 after four exact Undo
restorations. Warm scripted strokes take **516.00-599.14 ms**. Observed logical
CPU/GPU peaks are **102.25/32.11 MiB**. There is no separate M5 speed threshold
in the original roadmap and no real-time claim.

Windows 10 Pro; Xeon E5-2680 v3 at 2.50 GHz; 34,276,601,856 bytes RAM; NVIDIA
GTX 1650/Turing, driver 27.21.14.5751. Installed Chrome 156.0.8078.80 and Edge
156.0.4314.58, non-fallback NVIDIA WebGPU and NVIDIA ANGLE/D3D11 WebGL2,
1280x850/DPR1. Serial measurements with no builds/tests during timing, on a
shared machine. Software results establish function only.

The normal 6000x4000 document is shown at 100%, with populated visible tiles.
The fixture is an enlarged public-domain USAF portrait, cropped in Rust to
[0,600,1905,1270] and Lanczos3-enlarged without aspect distortion; it is not a
native 24 MP camera capture. PNG: 28,727,686 bytes, SHA-256
`9d3b70d5edb0cdf9939b0bc4cc57f6071128b125744b322e99fbeba577e66478`.

Each browser/API has a fresh context; each configuration has one first and three
warm calls. OS/file/kernel caches may already be warm. Brush size 200, flow/opacity 1,
spacing 0.1, mouse pressure 1, stabilizer off, no selection/mask/crop/locks. An original
64x64 asymmetric leaf PNG supplies the image tip. Texture/jitter adds an original
32x32 grain PNG, scatter 0.35, size jitter 0.3, angle jitter 0.7, opacity jitter 0.25,
depth 0.8, period 32 and seed 71. Mirror uses both axes; radial uses eight sectors,
center [3000,2000]. Asset hashes and native paths/settings are retained in the raw data.

Twelve native Playwright moves cover 240 px with a 20 px sine deviation, requesting 20 ms
between moves. Actual warm input delivery spans 22.02-26.32 Hz. Wall completion
includes automation/input, processing, observed committed presentation and
persistence. Opening, control changes, hash exports and Undo are outside timing.
Normal autosave stays enabled. No measured edit is retried; there are zero
outside-timing admission waits in this matrix.

Milliseconds: **first / warm p50 / warm p95**. With three warm trials, p95/p99
are the maximum observed warm value, not long-term tail estimates.

| Configuration  | Chrome WebGPU            | Chrome WebGL2            | Edge WebGPU              | Edge WebGL2              |
| -------------- | ------------------------ | ------------------------ | ------------------------ | ------------------------ |
| Image tip      | 548.32 / 536.63 / 550.66 | 556.03 / 558.74 / 563.72 | 545.40 / 556.36 / 580.41 | 568.87 / 548.58 / 563.92 |
| Texture/jitter | 545.12 / 544.82 / 554.92 | 566.11 / 541.66 / 561.57 | 520.51 / 536.24 / 540.23 | 561.85 / 565.96 / 568.54 |
| Both mirrors   | 528.54 / 534.77 / 539.64 | 568.43 / 564.31 / 582.49 | 573.83 / 523.99 / 541.76 | 573.61 / 563.68 / 569.65 |
| Radial 8       | 549.81 / 555.89 / 572.13 | 614.21 / 576.72 / 599.14 | 529.95 / 554.03 / 557.48 | 568.67 / 583.17 / 583.70 |

Warm changed-input acknowledgement and presentation-gap percentiles aggregate
the four configurations. Completion of all replicated dabs is tracked separately;
unchanged frames do not establish changed-pixel throughput. Acknowledgements
exclude compositor/scanout and physical input latency.

| Browser/API   | Changed ack p50/p95/p99 ms | Completed ack p50/p95/p99 ms | Presentation gap p50/p95/p99 ms |
| ------------- | -------------------------- | ---------------------------- | ------------------------------- |
| Chrome WebGPU | 15.19 / 38.51 / 57.81      | 17.30 / 57.02 / 84.62        | 18.02 / 29.14 / 58.46           |
| Chrome WebGL2 | 26.39 / 53.36 / 72.67      | 27.85 / 79.58 / 109.42       | 18.95 / 35.88 / 66.58           |
| Edge WebGPU   | 15.62 / 37.10 / 43.63      | 17.63 / 43.48 / 66.97        | 17.70 / 29.52 / 50.20           |
| Edge WebGL2   | 26.25 / 48.43 / 65.72      | 27.95 / 73.31 / 87.30        | 18.44 / 36.10 / 51.25           |

The fixed path and eight-sector support lie within at most nine tile coordinates.
Their conservative ordinary-paint coverage/output reservation is at most 4.5 MiB,
plus separately admitted tip/settings/queue/input, paging and display resources.
This bounds unobserved allocation without calling a frame-fence sample an
exhaustive peak. Raw data, ordinary-view captures, assets and summary:
`bench/results/p9-m5-axis-final-performance*`; reproduce with `pnpm bench:advanced-brush`.
Processing head is `8ea6566e4bd963a3df91b0ccab2c08689043122b`.

### Clone phase regression

Both complete source-mode matrices use the unchanged deterministic 6000x4000
67,038,649-byte fixture, SHA-256
`809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5`.
The ordinary 100% view uses a 200 px soft clone, offset [0,-180], opacity/flow 1,
spacing 0.1 and two 4-second warm native-input trials after a priming dab.
Requested input is 120 Hz/1800 px/s; actual delivery and acknowledgement traces are
retained. All-visible sampling paints a separate raster. No build/test overlaps.

| Source      | Browser/API   | Presentation fps, two trials | Changed-input ack fps, two trials |
| ----------- | ------------- | ---------------------------- | --------------------------------- |
| Current     | Chrome WebGPU | 57.08 / 57.69                | 49.60 / 44.46                     |
| Current     | Chrome WebGL2 | 57.00 / 56.86                | 48.79 / 42.65                     |
| Current     | Edge WebGPU   | 57.73 / 57.84                | 49.76 / 43.63                     |
| Current     | Edge WebGL2   | 56.65 / 56.86                | 48.17 / 42.89                     |
| All-visible | Chrome WebGPU | 27.93 / 30.14                | 24.44 / 24.91                     |
| All-visible | Chrome WebGL2 | 26.39 / 26.68                | 23.65 / 21.94                     |
| All-visible | Edge WebGPU   | 29.85 / 30.91                | 26.87 / 25.43                     |
| All-visible | Edge WebGL2   | 26.67 / 27.64                | 23.93 / 22.41                     |

Current/all-visible observed logical CPU peaks are 103.93/202.18 MiB; GPU at most
32.11 MiB. Current changed-ack p95 spans 25.54-30.62 ms, all-visible 50.10-64.63 ms;
the complete per-trial p50/p95/p99 cadence/latency, start response, source settings,
input delivery and bounds are in `p9-m5-axis-final-clone-*-performance.json` and the
performance summary. Presentation includes unchanged previews and does not mean
60 distinct document updates per second.

The original all-visible 60 fps target remains failed under its existing explicit
P12 deferral. **Current-layer misses remain open without a new user decision.**
An unmodified accepted M4 deployment-bundle comparison on the same machine today
also measures 56.08-57.74 fps; this is diagnostic context, not M5 acceptance or an
inferred extension of the old waiver.

## Verification, visual evidence and remaining gates

All 233 Rust tests, native/WASM clippy, Rust formatting, TypeScript, ESLint,
Prettier, 22 Vitest cases and 459 license expressions pass. Independent oracles
cover coverage/transform math, deterministic slicing, image/texture across every
stroke pipeline, selection/crop/alpha/locks, cold source pages, bounded admission,
partial refusal/cancellation/no-op redo and exact command restoration.

The clean 96-case focused installed-browser run and 42-case focused software run
pass once per case, with zero failures/skips/retries/flaky/global errors. Rebuilt
WebView2 passes both NVIDIA APIs, all retained broad checks and new image/preset/
oriented mirror/radial/coincident-union/one-command/exact undo/redo checks.
The optimization candidate's Windows WASM is 4,547,667 bytes / gzip9 **1,682,136**,
below the unchanged 2,000,000 gate. Full final verification is still required.

Ordinary-view captures and 4x nearest magnifications show transformed asymmetric
tip coverage and grain on both APIs. Magnifications composite onto light gray
for alpha inspection only; original exported samples remain unchanged. Existing
portrait seam/texture assessments retain their original scope and limitations.
Fresh phase-wide M2/M3/M4 regression images must be inspected with new evidence
prefixes before final certification.

Remaining final certification requires the complete 752-case local hardware
matrix, exact-final-head remote CI/deployment, actual downloaded artifacts
(expected 560 software cases and both software WebView2 APIs), hosted runtime
byte equality and relevant hosted hardware cases. Inspect all attempts; require
zero failures/skips/retries/flaky/global errors. The certificate records actual
run/artifact identities, hashes, case counts, driver events and source equivalence.
No passing subset substitutes for a complete matrix. Resolve the current-layer
performance gate before tagging, even if the other checks are green.

Failed/superseded attempts remain separate in `p9-m5-attempts.json`: the
Fit-versus-100% mask fixture, redundant input-residency walks, hidden-control
rerenders, a zero-test port conflict and the resulting shared trace-directory
collision. Exact assertions and deadlines were retained. A diagnostic 64-poll
GPU experiment did not improve speed and was reverted; the accepted nearest
sampling correction and completed-work fence remain intact. Instrumented and
old-runtime timings never certify the chosen runtime. A new independent oracle
found that coincident asymmetric orientations were discarded; Rust now unions
their coverage before flow. New native/browser/desktop center cases pass.
All pre-correction timings and the deliberately cancelled partial regression
matrix remain supplementary; acceptance uses complete corrected-source runs.
An isolated copied-build sampler experiment also showed no useful speed gain
and was rejected without changing the production shader.

## Deferred work and proposed P10

No selected M5 feature is deferred. Current-layer clone speed remains unresolved;
all-visible clone speed retains only its existing user-approved P12 deferral.
P5's identifiable Photoshop feather comparison remains open without `p5-select`.
Physical Windows Ink keeps its approved deferral. Recorded P6/P7 limits, P8's
approved P12 save-speed deferral and M3 fill-quality limits remain unchanged.

Proposed next: P10 Windows packaging, file integration and updater, with its
product decisions settled before implementation. Stop before starting P10.
