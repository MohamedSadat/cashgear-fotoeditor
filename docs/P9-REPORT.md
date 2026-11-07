# P9 - Retouch and advanced painting

P9 delivers Clone Stamp, healing/spot healing, content-aware fill, six retouch
brushes, editable gradients and advanced painting. M5 adds image tips, seeded
scatter/jitter, texture, CashGear brush libraries and mirror/radial painting.
Phase completion requires the remaining gates below and the immutable final
certificate before creating `p9-retouch`. P10 has not started.

**Current-layer clone speed remains open (2026-11-08):** the complete clean-source
matrix at `3e30cd0` measures 65.96-75.20 presentation fps on WebGPU and
53.25-59.99 on WebGL2. A complete same-source repeat, with agent tool activity
paused during timing, measures 69.39-72.44 and 48.11-59.73 respectively. Both
eight-trial matrices are retained. Other GPU applications remained uncontrolled;
their observed activity does not establish the cause of the stalls. The prior
November 4 matrix below is historical evidence, not current speed acceptance.
The user retained 60 fps; no additional speed waiver is authorized. The original
whole-window fps metric is unchanged. Changed-input acknowledgements, actual
GPU-fence completion traces, notification delays and baseline/tail gaps remain
separate from physical display scanout.

Opaque-row specialization, one shared document/history residency walk,
contiguous display conversion and 2D cursor transforms preserve exact samples
and budgets. The frame pump retains a preview deadline that fires during an
earlier GPU fence and services its unused slot after actual completion.
Fresh stroke input now uses a free slot immediately after GPU completion, even
within the same UI animation epoch. One GPU transaction remains in flight;
navigation and idle previews retain their animation pacing. Genuine input-driven
GPU completion rates can exceed display refresh, without implying faster scanout.
Stable control metadata also avoids rerendering editing panels for cursor and
memory telemetry, which still update at full rate. Instrumented CPU traces show
main-thread frame-message handling falling from approximately 2.05 to 0.69 ms per
message; those traces are not acceptance timings.
The earlier clean-source optimization still had a 59.25 fps WebGL2 trial;
that complete matrix and every rejected experiment remain separately classified.
All-visible sampling still misses 60 fps and retains only its existing P12
deferral. Full P9 acceptance is recorded by the final immutable certificate;
the completion tag is created only after every remaining gate passes.

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
between moves. Actual warm input delivery spans 20.82-26.40 Hz. Wall completion
includes automation/input, processing, observed committed presentation and
persistence. Opening, control changes, hash exports and Undo are outside timing.
Normal autosave stays enabled. No measured edit is retried; there are zero
outside-timing admission waits in this matrix.

Milliseconds: **first / warm p50 / warm p95**. With three warm trials, p95/p99
are the maximum observed warm value, not long-term tail estimates.

| Configuration  | Chrome WebGPU            | Chrome WebGL2            | Edge WebGPU              | Edge WebGL2              |
| -------------- | ------------------------ | ------------------------ | ------------------------ | ------------------------ |
| Image tip      | 620.24 / 579.97 / 581.77 | 552.22 / 547.34 / 580.46 | 618.27 / 547.16 / 573.15 | 553.33 / 543.42 / 576.27 |
| Texture/jitter | 594.73 / 566.03 / 577.51 | 525.00 / 561.13 / 579.54 | 509.92 / 563.63 / 579.10 | 544.18 / 558.79 / 628.35 |
| Both mirrors   | 567.34 / 577.58 / 583.76 | 506.12 / 578.60 / 614.51 | 591.39 / 579.01 / 579.65 | 536.30 / 542.68 / 595.05 |
| Radial 8       | 589.22 / 579.45 / 629.30 | 579.64 / 610.66 / 663.63 | 623.85 / 563.25 / 578.56 | 606.49 / 628.44 / 632.53 |

Warm changed-input acknowledgement and presentation-gap percentiles aggregate
the four configurations. Completion of all replicated dabs is tracked separately;
unchanged frames do not establish changed-pixel throughput. Acknowledgements
exclude compositor/scanout and physical input latency.

| Browser/API   | Changed ack p50/p95/p99 ms | Completed ack p50/p95/p99 ms | Presentation gap p50/p95/p99 ms |
| ------------- | -------------------------- | ---------------------------- | ------------------------------- |
| Chrome WebGPU | 11.88 / 45.63 / 65.30      | 12.58 / 58.75 / 88.79        | 17.42 / 26.21 / 55.21           |
| Chrome WebGL2 | 24.10 / 50.08 / 65.75      | 25.12 / 75.79 / 93.15        | 17.61 / 32.53 / 63.29           |
| Edge WebGPU   | 11.98 / 43.49 / 63.92      | 12.92 / 58.46 / 85.16        | 17.17 / 26.69 / 57.55           |
| Edge WebGL2   | 24.52 / 51.31 / 67.83      | 25.87 / 75.42 / 94.67        | 17.34 / 34.08 / 63.68           |

The fixed path and eight-sector support lie within at most nine tile coordinates.
Their conservative ordinary-paint coverage/output reservation is at most 4.5 MiB,
plus separately admitted tip/settings/queue/input, paging and display resources.
This bounds unobserved allocation without calling a frame-fence sample an
exhaustive peak. Raw data, ordinary-view captures, assets and summary:
`bench/results/p9-m5-frame-final-performance*`; reproduce with `pnpm bench:advanced-brush`.
This historical matrix uses processing head `4c7a643b7261b824dab8cec01b9c042ca77c5676`.
Fresh final-source measurements and build equivalence remain required.

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
| Current     | Chrome WebGPU | 59.98 / 59.98                | 51.27 / 44.80                     |
| Current     | Chrome WebGL2 | 59.99 / 60.00                | 51.28 / 45.06                     |
| Current     | Edge WebGPU   | 60.01 / 60.09                | 52.04 / 45.13                     |
| Current     | Edge WebGL2   | 60.05 / 60.00                | 51.58 / 45.06                     |
| All-visible | Chrome WebGPU | 29.64 / 29.91                | 25.90 / 24.43                     |
| All-visible | Chrome WebGL2 | 27.14 / 25.88                | 24.16 / 20.16                     |
| All-visible | Edge WebGPU   | 30.66 / 30.12                | 27.17 / 24.89                     |
| All-visible | Edge WebGL2   | 26.64 / 25.90                | 23.90 / 21.41                     |

Current/all-visible observed logical CPU peaks are 103.93/202.18 MiB; GPU at most
32.11 MiB. Current changed-ack p95 spans 24.41-30.35 ms, all-visible 51.17-60.28 ms;
the complete per-trial p50/p95/p99 cadence/latency, start response, source settings,
input delivery and bounds are in `p9-m5-frame-final-clone-*-performance.json` and the
performance summary. Presentation includes unchanged previews and does not mean
60 distinct document updates per second.

The original all-visible 60 fps target remains failed under its existing explicit
P12 deferral. The November 4 current-layer matrix restored its previously accepted
nominal 60 Hz range. The newer complete matrices above miss the retained target,
so the current-layer gate is open. Their raw evidence is in
`p9-m5-throughput-final-clone-current-performance.json` and
`p9-m5-quiet-window-clone-current-performance.json`.
A separately retained unmodified accepted M4 deployment-bundle comparison
also measures 56.08-57.74 fps; this is diagnostic context, not M5 acceptance or an
inferred extension of the old waiver.

## Verification and certification

All 233 Rust tests, native/WASM clippy, Rust formatting, TypeScript, ESLint,
Prettier, 27 Vitest cases and 459 license expressions pass. Independent oracles
cover coverage/transform math, deterministic slicing, image/texture across every
stroke pipeline, selection/crop/alpha/locks, cold source pages, bounded admission,
partial refusal/cancellation/no-op redo and exact command restoration.

Earlier focused runs retain 96 installed-browser hardware and 42 software cases,
plus a 124-case optimization-candidate hardware supplement. Each passes once,
with zero failures/skips/retries/flaky/global errors; these supplements do not
replace the final full matrix. Rebuilt
WebView2 passes both NVIDIA APIs, all retained broad checks and new image/preset/
oriented mirror/radial/coincident-union/one-command/exact undo/redo checks.
The optimized Windows WASM is 4,547,667 bytes / gzip9 **1,682,136**,
below the unchanged 2,000,000 gate. Final acceptance is guarded by the complete evidence requirements below.

Ordinary-view captures and 4x nearest magnifications show transformed asymmetric
tip coverage and grain on both APIs. Magnifications composite onto light gray
for alpha inspection only; original exported samples remain unchanged. Existing
portrait seam/texture assessments retain their original scope and limitations.
Fresh phase-wide M2/M3/M4 regression images must be inspected with new evidence
prefixes before final certification.

Final certification requires the complete 752-case local hardware
matrix, exact-final-head remote CI/deployment, actual downloaded artifacts
(expected 560 software cases and both software WebView2 APIs), hosted runtime
byte equality and relevant hosted hardware cases. Inspect all attempts; require
zero failures/skips/retries/flaky/global errors. The certificate records actual
run/artifact identities, hashes, case counts, driver events and source equivalence.
No passing subset substitutes for a complete matrix. The current-layer speed gate
must pass on the final source, with all eight raw trials retained, before
certification. No extra waiver is authorized.

Failed/superseded attempts remain separate in `p9-m5-attempts.json`: the
first unfiltered 752-case run failed three photographic retouch cases after
661 passes, leaving 88 unrun. Traces show Burn pointerdown during Autosaving;
the existing admission guard rejected the gesture before painting. The fixture
now saves the restored baseline before each photographic stroke. Normal
autosave, exact pixel assertions, deadlines and zero retries remain unchanged.
All four focused photographic hardware cases pass; a new full matrix is required.
Earlier attempts include the
Fit-versus-100% mask fixture, redundant input-residency walks, hidden-control
rerenders, a zero-test port conflict and the resulting shared trace-directory
collision. Exact assertions and deadlines were retained. A diagnostic 64-poll
GPU experiment did not improve speed and was reverted; the accepted nearest
sampling correction and completed-work fence remain intact. Instrumented and
old-runtime timings never certify the chosen runtime. The native centered-stamp fixture also exposed a stale redo snapshot: direct API
Undo had completed before React published its revision. The smoke now awaits
published redo and subsequent Undo revisions before new native input, keeping
the original exact pixel oracle. Failed exports and pointer coordinates remain
retained. A new independent oracle
found that coincident asymmetric orientations were discarded; Rust now unions
their coverage before flow. New native/browser/desktop center cases pass.
All pre-correction timings and the deliberately cancelled partial regression
matrix remain supplementary; acceptance uses complete corrected-source runs.
An isolated copied-build sampler experiment also showed no useful speed gain
and was rejected without changing the production shader.

## Deferred work and proposed P10

No selected M5 feature is deferred. Current-layer clone cadence is restored;
all-visible clone speed retains only its existing user-approved P12 deferral.
P5's identifiable Photoshop feather comparison remains open without `p5-select`.
Physical Windows Ink keeps its approved deferral. Recorded P6/P7 limits, P8's
approved P12 save-speed deferral and M3 fill-quality limits remain unchanged.

Proposed next: P10 Windows packaging, file integration and updater, with its
product decisions settled before implementation. Stop before starting P10.
