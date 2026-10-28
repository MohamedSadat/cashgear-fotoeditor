# Progress

## P9 - M2 Healing Brush and Spot Healing locally verified

Started 2026-10-28 at certified M1 `c630690977e1a3ca3a5558f2dd0d3ee78808540f`,
directly on clean main matching fetched origin/main, without worktrees or agents.
Read the phase records, M1 report/certification and complete original P9 roadmap
on printed page 17 as extracted text and a rendered page; its SHA-256 matches.
Existing product decisions answer all M2 questions. P9-DESIGN.md records M2
behavior and acceptance before implementation.

The implementation adds an original bounded Rust Poisson solver and deterministic
nearby patch search, immutable sampling, current/all-visible retouching, controls,
selection/locks/crop, one command and exact undo/redo. Stationary dabs reuse a
completed immutable patch after releasing numerical scratch. Pending jobs cancel
after pointer release; busy-worker Undo/Redo queue and execute exactly once.

All 196 Rust tests, native/WASM clippy, formatting, TypeScript, ESLint, 16 Vitest
tests and 459 license expressions pass. The final 48 focused installed
Chrome/Edge hardware cases pass once without skips/retries/flaky/global errors.
Rebuilt WebView2 passes both NVIDIA APIs and both tools' pixel/command/exact-
undo/redo checks plus the existing broad desktop smoke. WASM is 4,475,194 bytes /
gzip9 1,653,033 against the unchanged 2,000,000-byte gate.

The identifiable public-domain Melissa Diaz USAF portrait supplies an actual
blemish, inspected full before/after images, 100% captures and enlarged crops.
Both tools remove it without a visible seam; all samples outside the support are
byte-exact. Source/settings/hashes and the actual scope of the visual assessment
are in [P9-M2-REPORT.md](P9-M2-REPORT.md).

All 32 isolated 6000x4000 hardware configurations complete: 128 click strokes and
sixteen short paths. Warm 48 px clicks take 74.43-135.14 ms; 200 px clicks take
3.25927-3.90813 s. Large brushes remain slow; the roadmap has no separate M2 speed
threshold and no new exception is inferred. Logical CPU/GPU peaks are
198.26/32.11 MiB, with the separately admitted compositor allowance disclosed.
Repeated clone measurements retain the all-layer 60 fps miss and its existing
user-approved P12 deferral. No software result establishes hardware performance.

All 612 final installed Chrome/Edge WebGPU/WebGL2 hardware cases pass in 29.0
minutes, one attempt each, with zero skips/retries/flaky/global errors; complete
case records were inspected. All 36 focused local software cases also pass once
with the same clean result; they establish function only. Exact-head remote CI/deployment and downloaded-
artifact certification remain required before M2 acceptance. The deployment
workflow preserves its exact static bundle for hosted byte checks. The milestone
completion message and final certification JSON record the immutable remote
results. M3 and later milestones remain outside this work; no P9 tag.

## P9 - M1 Clone Stamp locally verified

Started 2026-10-27 directly on clean main at `af66e10` / remotely certified
`p8-files`, without worktrees or agents. Read AGENTS.md, PLAN.md, PROGRESS.md,
DECISIONS.md and P8-REPORT.md, plus the complete original P9 roadmap on printed
page 17 as text and a rendered page. The PDF hash matches the handoff.

All four P9 choices are answered: clone first, CashGear presets only, both
symmetry modes and editable multi-stop gradients. P9-DESIGN.md records full
scope, independently accepted milestone boundaries and M1 acceptance before
implementation. M1 delivers immutable Clone Stamp sampling, source picking and
indicator, aligned/non-aligned current/all-layer modes, bounded source hydration,
selection clipping and exact command undo. Healing and later tools remain pending.

All 185 Rust tests, native/WASM clippy, TypeScript, ESLint, 16 Vitest tests,
formatting, 459 license expressions and release builds pass. WASM is 4,460,038
bytes / gzip9 1,647,803 within the unchanged 2,000,000-byte gate. All 580 installed
Chrome/Edge WebGPU/WebGL2 hardware cases pass in 27.4 minutes, one attempt each,
with zero skips, retries, flaky cases or global errors. After a CSS-only compact-
control correction, sixteen Clone Stamp cases pass again and the controls fit
900 px and 1280 px viewports. WASM/worker processing hashes remain identical.
Rebuilt final WebView2 passes both NVIDIA APIs, Clone Stamp source/pixel/one-
command/exact-undo checks, six extended imports, thirteen PSDs, four exports
and the expanded recovery/geometry smoke.

The complete 16-trial hardware matrix measures current-layer presentation at
59.79-60.02 fps and all-layer sampling into a separate retouch raster at
25.89-32.61 fps. On 2026-10-27 the user explicitly accepted these M1 limits and
deferred the all-layer 60 fps target to P12. That target fails; other gates stay
mandatory. [P9-M1-REPORT.md](P9-M1-REPORT.md) records shipped/deferred work,
per-API cadence/latency percentiles, first-use/start response, source modes,
input delivery, memory and asset hashes. Superseded probes are retained and
separated from the final measurements.

Exact-final-head remote CI/deployment and downloaded artifact inspection remain
required before M1 acceptance; immutable results are recorded in the milestone
completion message. Implementation CI `40570505462` is green but contains two
retried clone fixtures, so it does not certify acceptance. Their traces show
input before the layer-operation presentation becomes idle. The fixtures now
await the expected revision/mask target and idle UI before clicking; pixel and
history assertions remain unchanged. All 36 local software cases (three complete
repetitions) and sixteen hardware cases then pass once each without skips,
retries, flaky cases or global errors. Runtime code is unchanged. A clean
final-head run remains mandatory.
The extended setup guard also awaits the first published frame snapshot, since
"Engine ready" can precede that attribute. Twelve software and sixteen hardware
clone cases pass with this wait, followed by all forty hosted clone/GPU cases
without skips, retries, flaky cases or global errors. Earlier failure evidence
is retained.
No software result establishes hardware performance.
Inherited P5/Ink/performance boundaries remain. P9 is still in progress, without
`p9-retouch`. Stop before M2 healing/spot healing after M1 certification.

## P8 - Files, color and raster PSD delivery

Implementation and local acceptance are complete. Completion is certified by the
annotated `p8-files` tag after exact-final-head CI/deployment and artifact
inspection pass; its annotation records the immutable evidence. The full report
is [P8-REPORT.md](P8-REPORT.md). Work stops before P9.

All 564 final installed Chrome/Edge WebGPU/WebGL2 cases pass in 26.8 minutes,
one attempt each, with no skips, failures, retries or global errors. The normal-
document hardware matrix completes sixteen dense 20-layer 6000x4000 saves in
43.276-60.282 seconds, with peak logical CPU 609.02 MiB and GPU 20.49 MiB.
All four retained archives have SHA-256 identity with the independently decoded
variants and preserve canonical document hash
`7e7073f7c09310418c777f351369f4df961d22429a01ccafe9b8165e65990db6`.
The original three-second target fails and is explicitly deferred to P12.

All 64 normal-document 24 MP exports complete within budgets. Per-browser/API
first/warm percentiles, output sizes and allocator/reservation measurements are
in the report; the complete helper allowance is distinguished from actual
allocator use. Final PNG/BMP/associated-TIFF ICC checks, thirteen independent PSD
oracles and six export decoder references pass. Hardware WebView2 passes both
APIs, six extended imports, thirteen PSDs and four export modes per backend.
Raw evidence: `bench/results/p8-final-*`, `p8-save-performance.json`,
`p8-export-performance.json`, `p8-tiff-*` and independent reference reports.
The deployed implementation also passes all 44 focused format/export hardware
browser cases once, with no errors/skips/retries, and retains COOP/COEP headers.
Evidence: `bench/results/p8-hosted-*`.
Implementation CI 40520015674 and deployment 40520015528 pass at `bee33f9`;
downloaded artifacts confirm 419 single-attempt software browser passes without
errors/skips/retries and both software WebView2 backends. Final-report-head
certification is recorded in the completion tag annotation.

### Earlier P8 checkpoints

The final TIFF audit corrects associated-alpha samples before ICC conversion and
extracts bounded ICC metadata even when it exceeds a tiny image's pixel buffer.
All 175 Rust tests, native/WASM clippy, formatting, TypeScript, ESLint and license
checks pass. The production WASM is 4,444,995 raw / gzip9 1,640,646 bytes. All 44
focused hardware browser cases pass once; rebuilt WebView2 passes both NVIDIA
APIs, six extended inputs, thirteen PSDs and four export modes per backend.
Independent PNG/BMP/associated-TIFF color checks have maximum RGBA error
`[1, 1, 0, 0]`. Evidence: `bench/results/p8-tiff-*`.

Remote CI 40507577141 and deployment 40507580612 pass at `7d62539`.
The downloaded artifact confirms all 413 software browser cases pass with no
skips, retries, flaky cases or global errors; desktop passes both software APIs.
This confirms the preview-grid correction. Final-source CI remains required.

The final import audit adds bounded BMP V5 ICC extraction; the base decoder
ignored V4/V5 color definitions. All 174 Rust tests, native/WASM clippy and the
standard build pass at 4,443,265 raw / gzip9 1,639,949 bytes. Independent
LittleCMS comparisons of PNG and BMP have maximum error `[1, 1, 0, 0]`, including
exact alpha/hidden RGB. All 40 focused format/export hardware browser cases pass
once, without errors/skips/retries. Rebuilt WebView2 passes both NVIDIA APIs,
five extended inputs, 13 PSDs and four export modes per backend. Evidence:
`bench/results/p8-bmp-icc-*`.

The completed initial 16-save/64-export matrix timed real document work but used
the fixed diagnostic onscreen scene. Screenshot review caught this harness error;
data/archives are retained under `p8-*-diagnostic-*` as superseded evidence. Final
runners use normal document mode and require populated visible tiles/refinement
completion. Their corrected matrices and full hardware regression remain gates;
the explicit P12 save-speed deferral is unchanged.
The corrected normal-document export proof completes all four formats with
visible document tiles and 16.1 MiB GPU residency; its screenshot shows the
actual 24 MP photo. Full corrected matrices remain required.

The integrated source is committed/pushed through `d05d5d1`. Its current worker
passes all 128 focused hardware file/export/PSD/viewport/GPU cases on one attempt
each, with no errors/skips/retries. The rebuilt expanded WebView2 smoke passes
both NVIDIA APIs, all 13 public PSDs and all four export modes. Evidence is
`bench/results/p8-files-current-*`.

Remote checks 40481412322 (`339bdde`) and 40487063891 (`d05d5d1`) fail the same
three high-zoom preview cases. The latter artifact has 410 other software cases
passing once, no skips/flaky/global errors; each affected case failed twice.
The test used a fixed zoom anchor tied to the old canvas height. The new recent-
files row reduces canvas height by 24 px, so 8x zoom shifts the small document
away from the sampled center. The fix uses the actual view center and keeps
the grid/identity-preview assertions. Local reproduction confirmed the cause;
all 20 installed-browser hardware and 15 software preview cases now pass once
without failures/skips/retries/global errors. Evidence:
`bench/results/p8-preview-grid-*`. New-head remote verification remains required.
The full software matrix takes 46.5 minutes before desktop work, so CI's job
envelope increases from 60 to 90 minutes without removing tests or backends.

Native export sources/codecs are committed at `f6d86ed`. The integrated export
candidate passes 173 Rust tests, native/WASM clippy, formatting, TypeScript,
ESLint, 16 Vitest cases and 459 license expressions. Release WASM is 4,440,303
bytes / gzip9 1,638,683. The sibling-worker integration passes all 124 focused
Chrome/Edge hardware cases once without failures/skips/retries/global errors.
The rebuilt WebView2 passes both NVIDIA APIs, including 13 PSD files and all
four export modes per backend. A subsequent file-cancellation reservation fix
passes all 28 export/file-access cases once, including real writable abort and
late-write-failure preservation of the old destination, pixels and redo.
Evidence: `bench/results/p8-export-*`. P8-EXPORT.md documents sources, memory,
codec limits and the observed WebView2 nested-worker failure/fixed sibling route.

Independent Pillow/libjpeg/libwebp checks validate six tagged/untagged exports:
both WebP alpha planes and lossless RGBA are exact, ICC presence/absence agrees,
and JPEG is opaque at 300 DPI. The independent large-project audit decodes all
7,680 LZ4 tile payloads and checks ZIP CRCs. Applying legacy canvas/guide defaults,
source and saved canonical document SHA-256 both equal
`7e7073f7c09310418c777f351369f4df961d22429a01ccafe9b8165e65990db6`.
See `bench/results/p8-large-canonical-reference.json`. The first audit draft
incorrectly included serialization-version/default-field differences; that
comparison was corrected without changing project data or samples.

Project output now batches bounded writes and records paging/encoding/write/yield
timings. The first isolated buffered Chrome/WebGPU save takes 51.952315 seconds:
25.314535 s paging, 13.118800 s encoding, 8.759465 s writes and 4.436985 s yields.
The user explicitly accepted the measured P8 limit and deferred the original
under-three-second target to P12. It remains a miss, not a passed speed target.
The complete hardware measurement matrix is still required.
Final full-suite hardware regression, current desktop cancellation verification,
all four performance combinations and exact-head remote checks remain required.

The PSD/file-access checkpoint passes 171 Rust tests, native/WASM clippy,
formatting, TypeScript, ESLint, 16 Vitest cases and 458 license expressions.
The standard release is 4,322,993 bytes / gzip9 1,588,906, within the explicitly
approved 2,000,000-byte P8 gate. All 146 focused installed Chrome/Edge cases pass
once across both hardware APIs, without failures/skips/retries/global errors.
The rebuilt WebView2 shell passes both NVIDIA APIs, including all 13 public PSDs
on each backend. Evidence: `bench/results/p8-psd-ui-final-*`.

Native PSD decoding is committed at `2cf5d5c`; worker/UI integration adds atomic
and cancellable imports, bounded worker-side File reads, File System Access
open/save/reuse and persistent recent handles with explicit permission handling.
The pinned MIT corpus includes independent layer/mask references and identifiable
Photoshop merged outputs; scoped results and refusal boundaries are in P8-PSD.md.
This does not validate P5 feathering or every Photoshop rendering feature.

Remote CI 40467804466 at `f589a0c` failed the desktop file-input test; deployment
40467806895 passed. The earlier hidden-input enabled check did not observe the
React race. The corrected smoke waits for the final crop snapshot and the visible
Open control before dispatching imports; the current hardware runs pass. New-head
remote verification remains required. Foundation `b881a1b` previously passed CI
with 329 single-attempt software cases and both software WebView2 APIs.

The real distinct dense 20-layer 6000x4000 save probe is substantially slower
than the under-three-second criterion: Chrome/hardware WebGPU trials took
65.407375, 65.599475 and 66.280645 seconds, producing 2,123,367,314-byte archives.
Observed logical CPU residency peaked at 611,624,942 bytes. The OS cache was not
cleared; these are first/warm trials, not disk-cold measurements. Evidence:
`bench/results/p8-save-performance-probe.json`. The later explicitly approved
P12 deferral is recorded above; all four browser/API measurements remain.

Started 2026-10-27 at the user's explicit fresh-session request, directly on clean
main at `86043ed` / remotely verified `p7-transform`, without worktrees or agents.
Read AGENTS.md, PLAN.md, PROGRESS.md, DECISIONS.md and the complete original P8
roadmap on printed pages 16-17, as text and rendered pages. The source PDF remains
unchanged. PLAN.md records the acceptance; P8-DESIGN.md records the investigation
and fixture/performance gates. Product and fixture questions were asked early;
the user selected PSD import only, BMP/GIF first-frame/8-bit TIFF, embedded sRGB ICC
by default and ZIP/LZ4 projects. No local Photoshop/fixtures are available; public
corpus investigation and actual validation remain required. See DECISIONS.md.

Independent work adds an adversarial legacy/streamed project corpus and a borrowed,
allocation-free PSD framing/resource inspector, plus a native candidate-fixture
inspection example. This is no PSD pixel decoder or Photoshop compatibility claim.
All 158 Rust tests, native/WASM clippy, Rust/JS formatting, TypeScript, ESLint,
16 Vitest cases, the 451-license inventory and production build pass. WASM is
unchanged at 3,820,283 bytes / gzip9 1,423,577. All 84 applicable installed
Chrome/Edge GPU/viewport/project/recovery/crop regressions pass on both hardware
APIs, one attempt each, with zero failures/skips/retries/global errors. The Windows
shell builds and expanded WebView2 smoke passes both NVIDIA hardware APIs. This
is an unintegrated I/O foundation's regression subset, not full P8 acceptance.
Raw evidence is `bench/results/p8-foundation-*`.

Seven pinned public PSD candidates have source/hash records. Six pass framing;
the 16-bit fixture is rejected. Creator/version metadata is recorded without
claiming validated Photoshop outputs. The real distinct dense 20-layer 6000x4000
fixture is generated and parsed as 7,680 cold tiles / 2,013,676,863 file bytes;
this is fixture validation, not the outstanding under-three-second browser save
measurement. See P8-DESIGN.md for source revision, hashes and exact limitations.

The export dialog/JPEG/WebP controls, save optimization/canonical large-file
validation, phase-wide regression and exact-head local/remote gates remain. No
`p8-files` tag. P5/Ink deferrals and recorded P6/P7 limits retain their status.

## P7 - Delivered; release evidence is recorded by p7-transform

Started 2026-10-25 at the user's explicit fresh-session request on clean main at
`5a22595`, the verified `p6-filters` head. Read the required phase records and the
complete original P7 roadmap on printed pages 15-16. PLAN.md now records the full
acceptance scope; P7-DESIGN.md records processing/integration/verification contracts.
The user answered all four questions: Lanczos3 default, non-destructive crop,
warp/puppet warp after v1 and content-aware scale outside v1. DECISIONS.md records
these choices.

### Final local acceptance (2026-10-27)

P7 implementation and local acceptance are complete; see P7-REPORT.md for shipped
work, numerical references, hardware tables, limitations and deferred work. The
annotated `p7-transform` marker is published only after the final commit's remote
CI/deployment and zero-failure/skip/retry artifact inspection. Its annotation
records that immutable completion evidence. P8 has not started.

All 64 Apply trials and 320 warm preview updates completed on the resident 24 MP
photo, sequentially on installed Chrome/Edge with actual NVIDIA WebGPU/WebGL2.
Warm downscale p50 is 2.75-2.86 seconds; exact move 3.18-3.26 seconds; selected
6 MP movement 6.19-6.58 seconds. Arbitrary 10-degree Lanczos3 rotation is slow:
warm p50 160.3-177.5 seconds. Warm preview medians are 16.2-23.0 ms; first preparation
and first updates are reported separately. Maximum measured Apply logical CPU
residency is 318 MiB; warm preview GPU residency is 35.5 MiB. These are observed
timings, not invented P7 latency thresholds or physical process-memory claims.

The engine build passes 153 Rust tests, native/WASM clippy, formatting, TypeScript,
ESLint, 16 Vitest cases and 451 license checks. Local WASM is 3,820,283 bytes /
gzip9 1,423,577. The 440-case full hardware run, 20-case crop follow-up and
116-case corrected-engine regression all pass on one attempt per case, with no
errors/skips/retries, covering 444 distinct hardware cases. Windows/WebView2 passes
both hardware APIs. The final global cancellation UI change passes 32 browser
cases on one attempt each and receives refreshed desktop verification before the
report commit.

The inspected `e188820` processing CI artifact has 329 passing software browser
cases on one attempt each, no errors/skips/retries, both software WebView2 APIs,
and WASM gzip9 1,423,641 bytes. CI run 40374982974 and deployment 40374982935 pass.
The final UI/report commit still requires its own remote verification before the
release tag. Existing P5 Photoshop-reference and physical Ink boundaries persist;
warp/puppet warp remain after v1 and content-aware scale outside v1.

### Earlier final-validation checkpoint (2026-10-26)

The complete 440-case installed Chrome/Edge hardware suite passes on one attempt
per case, with zero errors/skips/retries, in 21.7 minutes. The focused final crop
suite passes 20 cases, adding four exact-edge snapping regressions. A crop edge
already on a boundary now wins over nearby guides during perpendicular movement.
An older software selection assertion now waits for its frame's exact count;
three repeats pass. This covers 444 distinct hardware cases.

Initial 24 MP measurements revealed exact integer movement taking 65-76 seconds:
cofactor inversion perturbed exact unit coefficients and missed the tile-copy
path. Forward-matrix recognition now preserves that path and its exact inverse.
All 153 Rust tests and native/WASM clippy pass, including a new bounded tile-batch
and hidden-RGB regression. The corrected build passes all 116 applicable hardware
geometry/viewport cases and the rebuilt WebView2 smoke on both APIs. Formatting,
TypeScript, ESLint, 16 Vitest cases and 451 license checks pass. WASM is 3,820,283
bytes / gzip9 1,423,577. The corrected first Chrome/WebGPU move samples are about
3.1-3.3 seconds including storage/commit, with approximately 1.2-second kernels.
The full hardware benchmark remains in progress; no P7 timing target is invented.

The `cb46084` CI was cancelled at the 40-minute job envelope. Its partial artifact
also contained old masked-preview equality failures and a stale selection-frame
read that passed on retry; it is not accepted. Those checks are corrected and the
60-minute full software run is mandatory. Final report, full measurements,
exact-head remote CI/deployment and artifact inspection remain before tagging.

### Earlier crop authoring checkpoint

Adds the Crop tool, draggable/resizeable drafts, ratio presets, thirds, guide
snapping, Alt-drag movement and a drawn/numeric straighten angle. One bounded
whole-tree candidate rotates once and crops in one command; zero-angle crops
retain all original generations. Retained outside samples survive streamed
save/reopen; exact undo restores the pre-straighten document. Display mips/gutters
exclude hidden crop neighbors, and the checkerboard stays at the visible origin.
Worker replacement clears abandoned transform/crop control ownership.

All 152 Rust tests, native/WASM clippy, formatting, TypeScript, ESLint, 16 Vitest
cases and 451 licenses pass. Release WASM is 3,820,107 bytes / gzip9 1,423,475.
Sixteen crop cases, four actual worker-loss/restart cases and four masked rotation
preview/Apply comparisons pass across the installed Chrome/Edge hardware API
pairs. The latter compare within one channel value at a pixel-aligned camera
after rotated/flipped cache rebuild; P7-DESIGN.md records the separate rotated
atlas raster-precision limit. WebView2 passes both APIs with straighten, single
command, exact undo, streamed reopen and Reveal All checks.

An initial crop test helper read the snapshot before the first frame; it now
waits for an initialized snapshot. A worker-restart assertion now scopes the
duplicate Free transform controls to the tool rail. The first complete browser
run was superseded after revealing an unreliable after-step diagnostic: filling
the budget could succeed if a tiny job released scratch before its next allocation.
The diagnostic now refuses its allocation immediately after a real processing
step; real production budgets and exact atomicity assertions are unchanged.
Repeated software filter/refusal checks pass on both APIs. Raw/superseded evidence
is retained under `bench/results/p7-crop-*`, `p7-lifecycle-*`,
`p7-filter-diagnostic-*` and `p7-transform-rotation-*`.

Full browser verification, 6000x4000 hardware measurements and exact-head remote
gates remain. The expanded software suite's job envelope is 60 minutes; the
preceding remote run had already spent over 30 minutes in build and full software
checks. This changes no individual test assertion or missing-backend requirement.
No `p7-transform` tag yet.

### Earlier immutable transform and Move checkpoint

Adds immutable free-transform sessions with eight handles and a rotation handle,
scale/rotate/skew/distort/perspective controls, Shift proportions, Enter/Escape,
live GPU previews, one final full-resolution Rust resample, Move dragging,
optional layer auto-selection, arrow nudging and guide/canvas snapping. Pageable
exact occupied bounds avoid sparse-padding growth. Linked masks move with the
whole layer; selected coverage is lifted/cleared/composited once. Invalid matrices,
locks, cancellation and budget refusal retain samples/history/redo. View changes
rebuild the preview against the same source and preserve the visible screen anchor.

All 150 Rust tests, native/WASM clippy, formatting, TypeScript, ESLint, 16 Vitest
cases and 451 dependency licenses pass. Release WASM is 3,813,445 bytes / gzip9
1,421,962. Twenty-four installed Chrome/Edge transform/API/control cases and eight
additional masked-preview/view/cancellation/refusal cases pass, one attempt each.
All 116 applicable GPU/viewport/crop/size/filter/adjustment-preview regression
cases also pass on the installed Chrome/Edge API pairs, without skipped/retried
cases. The exact filter-preview screenshot passes in both browsers/backends.
The Windows shell builds and WebView2 passes both APIs, including immutable
inverse rotations, a single move command and exact undo. These small fixtures are
functional evidence, not the outstanding 6000x4000 P7 performance measurement.
Raw evidence is `bench/results/p7-transform-*`.

The added screenshot initially included the overlapping DOM handles before Apply
and their removal after Apply. Hiding only that overlay for the pixel comparison
gives exact GPU preview/committed equality in the rotated/flipped masked-group
fixture. The budget-after-step test now uses multiple tiles so refusal occurs
before completion. Superseded reports/traces remain recorded. The earlier remote
`5e3dd87` CI had a retry and `929cbc3` CI failed the same filter-preview screenshot.
Trace evidence showed its final screenshot could precede the committed frame.
That test now waits for the committed content revision and inactive preview before
its existing two animation frames; the exact pixel assertion remains unchanged.
Neither old CI run satisfies the clean-attempt acceptance gate.

Crop gestures/ratios/straightening/thirds, crop-edge mip isolation, hardware P7
measurements and final full local/remote gates remain. No `p7-transform` tag yet.

### Earlier resampling and size checkpoint

This increment adds bounded Rust nearest/bilinear/bicubic/Lanczos3 sampling,
image size/DPI, anchored canvas size, canvas rotation/flipping and their controls.
Whole-tree jobs include nested rasters, masks, selection and guides, publish one
exact document command, and hydrate/yield/persist through the existing pager.
Identity shares generations; integer permutations preserve exact straight samples.
Image resizing uses fast_image_resize 6.1.0 with temporary premultiplied float
blocks; the approved license inventory now contains 451 dependencies.

The independent image 0.25.10 reference at 50% Lanczos3 measures maximum RGBA
error `[1, 0, 1, 0]` on the recorded 1030x518 opaque fixture. All 139 Rust tests,
native/WASM clippy, formatting, TypeScript, ESLint and 16 Vitest cases pass.
Release WASM is 3,724,728 bytes / gzip9 1,392,252. Sixty installed Chrome/Edge
GPU/viewport/crop/size cases pass on one attempt each without skips or retries;
12 size cases pass again after the final control layout review. The Windows
shell builds and hardware WebView2 passes both APIs, including exact canvas
rotation/resize/undo and the existing crop/reopen and P6 checks. Raw evidence is
`bench/results/p7-resample-*` and `p7-lanczos-reference.log`.

The first browser reference incorrectly compared retained columns outside the
visible export window. The corrected test uses Reveal All before checking every
original sample, then verifies undo/redo and two flips exactly. The superseded
report and trace remain under `p7-size-export-window-fixture-failure*`.
At that checkpoint sessions/GPU preview, move/nudge/auto-select, crop gestures/
ratios/straightening/thirds, snapping, full performance and final gates remained.

### Earlier canvas-window checkpoint

The canvas-window increment adds metadata-only non-destructive crop, Reveal All,
exact whole-document geometry commands, editable persistent guides, ruler/guide
overlays, visible-window PNG export and schema-4 streamed project/recovery metadata
with schema-2/3 reading. Brush/eraser, fill/clear, copying, paste centering, wand
membership/AA and Select All respect the visible canvas. View fitting, coordinate
conversion, selection display and viewport/preview clipping retain backing coordinates.

All 128 Rust tests pass, including hidden RGB/mask retention, odd crop exports,
crop-aware painting/fill/copy/wand, metadata refusal, no-op redo and exact history.
Native/WASM clippy, formatting, TypeScript, ESLint, 16 Vitest cases, all 450 license
expressions and the production build pass. WASM is 3,632,066 bytes / gzip9 1,358,619.
Eight Chrome/Edge crop/recovery cases and 100 applicable GPU/viewport/selection/
preview/layer-recovery regression cases pass without skips or retries. The expanded
hardware WebView2 smoke passes crop/guide streamed reopen and exact Reveal All on
both APIs. Final overlay pixel checks pass on all four installed browser/API
pairs, and the Windows build/smoke was refreshed against the corrected web output.
This remains partial P7 coverage.

Visual review corrected checkbox wrapping and explicit overlay stacking above the
GPU canvas. Crop inputs reset relative to the new visible origin. An added preview
fixture originally passed an absent active-layer ID; it now falls back to the first
raster, as the engine does. Its failed report/trace remain under
`bench/results/p7-canvas-preview-test-fixture-failure*`. A zero-width vertical SVG
path cannot use Playwright's rectangle visibility assertion; the overlay check
instead inspects finite geometry and the painted guide pixels, including the
specified guide color's anti-aliased blend over the fixture's red background.
The superseded unblended-color expectation is retained in
`bench/results/p7-canvas-overlay-antialias-reference-failure.json`. Samplers, transform
sessions/GPU preview, move/nudge/auto-select, size/rotation/flip dialogs, crop gesture/
ratios/straightening/thirds, snapping, full P7 performance and final gates remain.

### Earlier geometry checkpoint

The first independent increment adds
validated affine/projective Rust geometry and the four interpolation names,
without selecting a default or changing document samples. Four geometry tests
include 500 randomized inverse round-trips; see P7-DESIGN.md for the exact scope.

All 119 Rust tests, native/WASM clippy, Rust/JS formatting, TypeScript, ESLint,
16 Vitest cases, the 450-dependency license inventory and the production build
pass. Release WASM is 3,588,418 bytes / gzip9 1,344,806 bytes. All 40 installed
Chrome/Edge GPU/viewport regression cases pass on WebGPU/WebGL2, each on exactly
one attempt, without failures, skips, retries or global errors. This is the
applicable regression subset for unintegrated geometry, not full P7 acceptance.
The Windows shell builds and WebView2 155.0.4283.62 passes the existing expanded
smoke on hardware WebGPU (NVIDIA) and WebGL2 (NVIDIA GTX 1650 / D3D11). Raw evidence
is `bench/results/p7-foundation-{rust-tests.log,browser-tests.json,desktop-smoke.json,
wasm-size.json}`. No P7 pixel-resampling, live-transform preview, crop integration
or hardware performance is claimed yet. This checkpoint is committed on main;
remote checks are not P7 phase acceptance.
The geometry checkpoint `004055d` passes CI 40209644112 and deployment 40209644178. Inspected artifacts contain 272 software browser cases with exactly
one passing attempt each, zero skips/retries/failures/global errors, and both
software WebView2 backends. CI gzip9 WASM is 1,344,837 bytes. The artifact is
retained under `bench/results/p7-foundation-ci`.

P7 implementation and acceptance remain in progress; no `p7-transform` tag.
P5's Photoshop feather reference is still pending without `p5-select`; the
physical-pen deferral remains unchanged. No worktree or agents are used.

## P6 - M4 delivered; phase release gated by final verification

Resumed 2026-10-25 in the explicitly requested fresh session from accepted M3
`e24b231`. Read AGENTS.md, phase records, all P6 milestone reports and the original
roadmap P6 section on printed pages 14-15. M3 CI 40171088637 and deployment
40171088675 are green. Work stays on main without a worktree or agents.

All twelve adjustments and ten filters now have Rust processing, GPU previews,
controls, exact commands and persistence. The seven remaining color controls
are destructive or editable layers (`32aef5e`, `92bc9a2`); five remaining spatial
filters are in `073d008`. All 115 Rust tests, 16 Vitest cases, native/WASM clippy,
formatting, TypeScript, ESLint and the 450-expression license inventory pass.
The optimized release is 3,588,418 bytes / gzip9 1,344,806 bytes.

The initial 144 M4 feature cases, full 368-case deployed hardware suite and
272-case remote software suite passed without failures, skips or retries.
Both hardware and software WebView2 APIs passed the expanded smoke. Initial
CI 40177491873 and deployment 40177491972 were inspected at `a418109`.
Supplemental probes then found the M3 frame timestamp used completion instead
of submission time. `c53b386` corrects that slot accounting while preserving
the completion fence, captured snapshot and preview acknowledgement, and busy
scheduling. All unit checks and eight focused hardware admission/preview cases pass.

Final-worker hardware measurements pass all 24 Gaussian radius-20 applications
on the resident 6000×4000 photo (maximum Apply 1.55143 s) and all 2,240 warm
control events (maximum 81.21 ms). Apply peaks at 304 MiB logical CPU; filter
preview peaks at 105.063 MiB CPU / 26.616 MiB GPU. First-use shader work is
separate and can exceed one second. Supplemental steady navigation measures
59.25–60 fps; brush throughput is about 60 fps WebGPU / 48–51 fps WebGL2.
Completed-GPU acknowledgement latency exceeds one frame; this is not a new
pass of P2's earlier submission-latency target. Unrelated archive CPU test jobs
were observed on the shared host; final runs are serial, not dedicated-host claims.
See P6-M4-REPORT.md for tables, contracts, failed/superseded evidence and limits.

The annotated `p6-filters` tag is the phase completion marker and records the final
head, CI/deployment and verification evidence. It is pushed only after final-head
CI/artifact inspection, all 368 deployed hardware cases without retries/skips,
both hardware WebView2 backends and delivery checks pass. No P7 work has started.
P5 Photoshop feather comparison remains pending without p5-select; physical pen
validation retains its explicitly approved deferral. The proposed next phase is
P7 transforms, crop, resampling and guides; stop before starting it.

## P6 — M3 implementation and local acceptance complete; phase in progress

Started 2026-10-24 in the explicitly requested fresh session, on clean synchronized
main at `77ec974`. M2's final CI 40115430175 and deployment 40115430197 both passed.
Read the phase records and original roadmap P6 section on printed pages 14–15.
Existing product decisions apply; no worktree or agents were created.

Gaussian/box blur, unsharp mask, sharpen and median now have independent Rust
kernels, bounded tile halos, selection clipping, exact commands and Fade jobs.
GPU viewport preview, controls and worker/paging integration are implemented.
Fade retains separate full and selected endpoints so blend changes apply
fractional selection coverage once. The latest 68 hardware feature cases pass,
including 56 complete CPU/GPU tile comparisons per browser/backend. All 107 Rust
tests, native/WASM clippy, Rust/JS formatting, TypeScript, ESLint, 16 Vitest cases
and the 450-expression license inventory pass.

All 24 measured Gaussian radius-20 applications on the resident 6000×4000 photo
pass: worst API-return p95 1.512 seconds; visible p95 at most 1.715 seconds.
All 800 fitted/actual-pixel warm slider events pass 100 ms, with worst p95
64.60 ms and maximum 81.46 ms. Actual-pixel API cases include maximum convolution
and median radii. Initial source/GPU preparation is recorded separately. Logical CPU
peak is 304 MiB for Apply; preview peaks at 105.063 MiB CPU / 26.616 MiB GPU.
These are serial hardware measurements on verified NVIDIA APIs, not software
results. Release WASM is 3,485,823 bytes / gzip9 1,318,717 bytes.

The initial 3.1–4.0-second Gaussian misses, WASM heap-growth profiling, later
2.17-second Edge miss and opaque/SIMD correction are disclosed in P6-M3-REPORT.md.
That report also records the corrected WebGL2 median translation and stale test
baseline. Superseded raw evidence remains under `bench/results/p6-m3-*`.

The current full 164-case software regression and 68 hardware feature cases pass
without failures, skips or retries, as do both hardware and both software
WebView2 APIs. The preceding worker also passed all 224 deployed hardware cases.
The final deployment repeats that full suite. Earlier software fixture timing
failures and their trace-based corrections are disclosed in the report.
Final deployed hardware regression, pushed CI/deployment and artifact inspection are the remaining
milestone gates; the completion message names their immutable run links.
The first pushed CI run passed 146 cases, timed out the combined five-filter
workflow and retried two selection fixtures. Trace-based corrections isolate
each filter case and await selection input admission, preserving all sample
assertions and existing timeouts. The expanded suites contain 224 hardware and
164 software cases; their final results and exact-head CI remain required.
All 48 deployed hardware adjustment/filter feature cases and production
isolation/WASM delivery checks passed before the final scheduling correction.
The second CI run passed 163 cases, with one older adjustment workflow blocked
by admission/Autosave and then timing out on retry. Ordinary viewport redraws
now pause during busy command work, while a prepared preview can still present.
The changed worker passes 68 hardware and 102 repeated software feature cases,
all 107 Rust tests and static/unit/license checks. Performance, full regression,
desktop and exact-head CI/deployment evidence are refreshed before acceptance.
The third CI run passed 163 cases and exhausted the older selected-raster
preview workflow's deadline. Its trace confirms cancellation completed, with
19-58-second screenshots and a 31-second DOM read on the hosted software GPU.
The frame pump now waits for submitted GPU work before releasing its RPC slot,
bounding animated-selection submissions on slow adapters. Final checks and
measurements are refreshed for this change. Four zoom-only fixtures also await
their acknowledged revision; the first local fence run's stale baseline is
preserved with its trace. Frame snapshots describe the submitted pixels, and
an older frame cannot acknowledge a newly prepared preview during its wait.
See the report for all failed runs and diagnostic evidence.
P6 is in progress without `p6-filters`; M4 is not started. Its
proposed scope is the remaining seven adjustments and five filters, followed by
phase-wide validation. Stop before M4. The P5 Photoshop feather comparison and
physical pen deferral retain their existing status.

## P6 — M2 implementation complete; phase in progress

Resumed 2026-10-24 at the user's request. The five priority adjustments now have
editable layer content and exact metadata commands, masks/groups/clipping/blend
integration, schema-3 streamed persistence/recovery, and opt-in GPU viewport
preview. Visible source caches preserve paging and budgets; preview never edits
document/history, and final confirmation covers the full-resolution selection.
See P6-M2-REPORT.md and P6-DESIGN.md for processing contracts and measured limits.

All 96 Rust tests, 16 Vitest tests, native/WASM clippy, Rust/JS formatting,
TypeScript, ESLint, licenses and the release size gate pass. The 184-case hardware
regression, final 32 feature cases, 134-case software regression plus three added
grid cases, and both hardware WebView2 APIs pass without skips/failures/retries.
The final clipping-base correction also passes 36 hardware feature/compositor
cases, including 400 adjustment scenes per browser/API. An adjustment clipping
base now inherits backdrop alpha and retains its blend function.
The corresponding six software compositor cases also pass.
A superseded CI run retried a software brush-boundary fixture after protocol
stalls. Its finite queued-input correction passes nine software repeats and
four installed-browser hardware cases; P6-M2-REPORT.md records the evidence.
The next full software run exposed a lock-test click during recomposition (136
passed, one failed). The trace-confirmed admission wait passes all 12 repeated
software layer-control cases; the final CI gate includes this correction.
Final pushed CI runs the 137 software cases together and both software WebView2
APIs; green CI/deployment are required before the task reports M2 accepted. The
task completion message names their immutable run links. The earlier layer
foundation already passes CI 40105107961 and deployment 40105108054.

On the resident 6000×4000 photo, all 160 measured warm slider events complete in
under 35 ms, with worst p95 31.70 ms. Initial readiness, including source/GPU work,
is 0.33–0.38 s on WebGPU and 0.92–0.98 s on WebGL2. Sampled logical residency is
at most 104.0 MiB CPU / 22.49 MiB GPU. Release WASM is 3,382,747 bytes / gzip9
1,279,600 bytes. Navigation remains about 60 fps; brush acknowledgement p95 is
at most 16.29 ms on the measured workload. These are hardware measurements;
software checks supply functional evidence only.

M2 implementation commits: `5705cf7` (layers/persistence), `2b537b6` (source
cache), `398cc36` (engine), `683f863` (controls). P6 remains incomplete, without
`p6-filters`. Proposed next is M3's first five filters, padding/selection clipping,
preview and Fade Last Filter. Stop before beginning M3. P5's Photoshop feather
comparison remains pending, without `p5-select`; the pen deferral is unchanged.

## P6 — M1 complete (history)

Started 2026-10-24 on clean main at `27576cb1`, by the user's explicit fresh-session
request. Read AGENTS.md, the phase records, P5 integration/report and the complete
original P6 section (printed pages 14–15, text plus rendered pages). No worktree
or agents. The four product choices are recorded in DECISIONS.md; P6-DESIGN.md
defines independently accepted milestones. M1 covers the five priority destructive
adjustments and active-layer histogram. GPU preview/adjustment layers, filters and
the other adjustments remain later P6 milestones. M1 acceptance is complete;
P6 as a whole is not complete and `p6-filters` has not been created.

The verified implementation is pushed through `7a845a1`. It adds bounded Rust
brightness/contrast, levels, master RGB curves, exposure and hue/saturation,
selection-clipped atomic commands, exact undo/redo and an active-raster histogram.
The shared CPU/GPU budgets, paging, straight alpha/hidden RGB and file caps remain.
See P6-M1-REPORT.md for shipped/deferred scope and the numerical/measurement limits.

All 87 Rust tests, 16 Vitest tests, native/WASM clippy, formatting, TypeScript,
ESLint, licenses and the production size gate pass. The 156-case installed
Chrome/Edge hardware regression and 113-case local software regression pass;
final UI follow-ups pass all 12 hardware and nine software adjustment cases.
All 12 final public-site cases and both hardware WebView2 APIs pass. The final
[CI run 40091443027](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40091443027)
passes all 113 software browser cases with zero failures, skips or retries and
both software WebView2 APIs. [Deployment 40091443033](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40091443033)
also passes. Earlier test timing/admission corrections are disclosed in the report.

On the 6000×4000 photo, worst warm API-return p95 is 1.999 s for Exposure and
2.385 s for HSL; histogram worst p95 is 0.625 s. Sampled logical residency peaks
at 289 MiB CPU and 16.11 MiB GPU. These are single-worker full-resolution apply
measurements, not slider-preview or Gaussian-blur acceptance. Existing navigation
measures 59.75–60.00 fps; brush pointer acknowledgement p95 is at most 14.68 ms.
Local gzip9 WASM is 1,244,718 bytes; CI is 1,244,781, both under 1,500,000.
Raw completion evidence: `bench/results/p6-m1-final-verification.json`.

M2 resumed by the user on 2026-10-24: adjustment layers, CPU/GPU parity,
persistence and GPU viewport preview. P7 remains outside the current phase.

The subsequent documentation commit `37dbe92` deployed successfully, but its
CI run 40092450448 exposed selection-test scheduling failures (111 passed,
one failed, one flaky). The first attempt collided with autosave admission;
the retry trace also shows software-driver stalls stretching native click
delivery and UI reads into tens of seconds. The follow-up shares the existing
pre-mutation admission retry helper and bounds the single-dab software fixture
to one down/up task; hardware keeps native input. All 18 repeated software
selection cases, eight installed Chrome/Edge hardware cases, formatting, lint,
TypeScript, 16 Vitest cases and the web production build pass locally. Remote
verification of this correction passed in CI run 40103488769; deployment
40103488794 also passed. This does not change phase status.

M2's first implementation increment introduces validated adjustment-layer
content, insertion/edit commands, exact metadata undo, selection-derived sparse
masks, schema-3 manifests within the existing bounded v2 framing (with schema-2
read support), and shared CPU/WGSL composition. All 91 Rust tests, native/WASM
clippy, JS checks, formatting, 16 Vitest tests, licenses and production build
pass. The new hardware comparisons cover 320 scenes on each installed
Chrome/Edge API, including all five settings, 16 blend modes and four mask/group/
clipping scenarios. A Color Burn/HSL endpoint mismatch found by these comparisons
was corrected by quantizing the adjustment blend inputs consistently. At that
increment, the UI, viewport preview, recovery-specific scenarios and M2 acceptance
were still in progress; the current M2 status is recorded above.

P5's identifiable Photoshop feather comparison remains pending. Starting P6 does
not close or waive it; `p5-select` remains uncreated.

## P5 — In progress

Started 2026-10-24 on clean main at `9b03c160`, the accepted `p4-layers` tag.
Verified the remote branch and tag, plus green checks/deployment for that commit
(runs 39946444773 / 39946444912). Read AGENTS.md, all phase records, P4-REPORT.md
and original roadmap printed page 14. No worktree or agents were created.

The user confirmed Quick Mask, anti-aliasing on by default, AI Select Subject
outside v1 and new-layer paste. No Photoshop reference/installation is currently
available; its visual feather acceptance remains pending, without an exception.
P5-DESIGN.md records the implementation and validation contracts. P5 is not
complete and `p5-select` has not been created.

The first Rust foundation adds shared Gray8 selection state to document/history
residency and paging enumeration, exact selection commands, rectangle/ellipse/
polygon masks, coverage algebra and selection-clipped brush/eraser strokes.
The native/WASM clippy checks, all 65 Rust tests, TypeScript, ESLint, 14 Vitest
tests, formatting, the 450-expression license inventory and production build
passed. Release WASM is 3,093,861 bytes / gzip9 1,178,584 bytes. This is an
internal foundation; selection UI, overlays, fill/clipboard and P5 browser/
hardware/desktop acceptance remain unfinished. Existing generated benchmark
output is now excluded from ESLint as it already is from Git and Prettier.

The next internal checkpoint adds bounded tilewise matching, four-connected
scanline flood selection, anti-aliased masks, scanline polygon rasterization,
invert/feather/grow/shrink/border/smooth and exact selection-clipped fill/clear
jobs. All 70 Rust tests, native and WASM clippy, formatting, licenses and the
production build pass. WASM is 3,093,875 bytes / gzip9 1,178,626 bytes; these
processing jobs are not bridged into that WASM build yet. No P5 browser,
clipboard, overlay, hardware or Photoshop acceptance is claimed by this checkpoint.

The integrated engine and UI now include the P5 tool set, bounded fill/clipboard
jobs, Quick Mask and GPU ants, session-only selection history, exact clipboard
tokens and visible system-API fallback. All 79 Rust tests, native/WASM clippy,
formatting, TypeScript, ESLint, 16 Vitest tests and licenses pass. Release WASM
is 3,229,550 bytes / gzip9 1,231,998. Windows/WebView2 passes both APIs, including
the new selection/fill/cut/paste/undo and Quick Mask pixel-isolation workflow.

The final 6000×4000 NASA-photo wand workload passes on all four Chrome/Edge
hardware APIs: maximum API return 164.42 ms, visible acknowledgement 188.20 ms,
2,364,448 connected / 2,970,734 global matches at tolerance 32, AA on. Independent
navigation and 300-pixel brush regression measurements are recorded in
P5-REPORT.md. All 144 local installed Chrome/Edge hardware tests pass, with no
failures, skips or retries, including all 36 P5 cases. The implementation and
verification are pushed on main; remote verification is linked from
the phase report. The unavailable Photoshop feather comparison is still pending;
prepared deterministic reference-input masks do not close that acceptance item.

## P4 — Complete

Started on 2026-10-23 on clean main at `ae46c5b`, the tagged P3 completion.
Read AGENTS.md, the phase records and original roadmap printed page 13. The user
confirmed sRGB, default pass-through groups, after-stroke thumbnails, and the real
50-dense-layer workload with paging and a 4 GiB streamed project cap. CPU/GPU and
image-file budgets are unchanged. P4-DESIGN.md records the implementation and
acceptance plan. P4 acceptance is complete; its completion tag is `p4-layers`.

The final implementation and verification are pushed through `4398ae1`. The editor now
has paged layer/group/mask editing, active paint targets, locks, merge operations,
thumbnails, float32 GPU composition, and streamed manual/recovery projects. See
P4-IMPLEMENTATION.md for the contracts and resource limits. All 108 installed
Chrome/Edge hardware cases passed without failures, skips or retries; all sixteen
modes were compared with the CPU reference on both APIs. Extended WebView2
verification also passed: both APIs passed blend and small streamed roundtrips;
WebGPU saved/reopened the full 2.52 GB document with identical exported PNG hash.
All 108 public-site hardware tests also passed, with zero retries, skips or
failures. No P5 work has started.

All four actual dense-workload runs are complete. The 2,517,098,823-byte fixture
has 50 distinct layer hashes and 9,600 distinct tile payload hashes. Three warm
pan trials per browser/API measured 59.73–59.99 fps at fitted 16.4667% zoom, with
period p95/p99 tails up to 33.40 ms. Cold copy/validation/composition took
89.47–192.15 seconds; thumbnails and first recovery extended setup to
202.73–330.41 seconds. Logical CPU was bounded by 368.25 MiB (sum of category
maxima), GPU by 20.34 MiB. Working storage plus one completed recovery used
5,034,199,330 bytes. P4-REPORT.md gives the hardware, dataset, timing method,
per-backend results and limitations.

[FotoEditor checks run 39944086502](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39944086502)
and [Azure deployment run 39944086415](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/39944086415)
passed for `4398ae1f162763631251ed769618d89cf4d7f0c3`. CI passed all 77 software
browser cases without retries, skips or failures, all required Rust/JavaScript
checks, and the extended WebView2 smoke on both software APIs. Its artifact is
retained under bench/results/p4-ci-complete. CI WASM is 3,092,274 bytes, gzip9
1,178,061 / 1,500,000. These software checks establish function; hardware
performance comes from the measured NVIDIA runs above.

The earlier integrated CI run `39939070886` failed in a desktop download hook
that captured a thumbnail PNG as project data. The corrected hook filters project
MIME and verifies the file magic; the successful run above includes that fix.

Final single-layer regression opens measured 1.13–1.31 s initially and
0.95–1.11 s subsequently. Pan/zoom measured 59.998–60.000 fps; cold mip refinement
after stopping took 1.12–1.40 s, improved from the earlier foundation checkpoint
below. P4-REPORT.md contains the complete phase report, shipped behavior, limits,
deferred physical-pen exception and raw-evidence references. No P4 feature is
deferred. Proposed next phase: P5 selections, fills and clipboard. Stop before
starting it.

### Earlier foundation checkpoint (`48377d6`)

The following evidence records that earlier checkpoint, before integration:

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

Outstanding at that foundation checkpoint were worker OPFS tile backing and eviction,
streamed manual project and recovery integration, GPU compositor execution and
CPU/GPU golden readbacks, incremental composite caching, layer controls/selection/
drag reorder, mask painting/view/apply, merge operations and thumbnails. The live
application still exposes its single-raster controls and v1 128 MiB project path;
the approved 4 GiB layered format is a Rust adapter, not a shipped UI feature yet.
The actual distinct dense 50-layer workload was then unverified. No sparse/shared
fixture substitutes for that acceptance target. Current integration work and
verification are recorded above.

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
