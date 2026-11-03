# P9 M4 - Retouch brushes and gradients

M4 delivers Smudge, Blur, Sharpen, Dodge, Burn and Sponge brushes, plus five
gradient shapes and an editable multi-stop preset library. Completion is
certified by `bench/results/p9-m4-final-certification.json` only after the full
final-head local, remote and hosted gates pass. This milestone does not complete
P9, authorize M5 implementation or authorize `p9-retouch`.

Started 2026-11-02 directly on clean main at certified M3
`2ab9dbdd6820b23196cb1c5b2f1f4b0baab13d11`, matching fetched origin/main, without
agents or worktrees. Read AGENTS.md, PLAN.md, PROGRESS.md, DECISIONS.md,
P9-DESIGN.md and P9-M3-REPORT.md. Re-read the complete original printed page 17
as extracted text and a newly rendered page. Original PDF SHA-256 remains
`83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.
Existing product choices answer every phase question. M4 behavior and acceptance
were recorded in P9-DESIGN.md before implementation.

M3's immutable certification remains unchanged, SHA-256
`60c6b6789622908e6e82d9a5ed889bec9f1c1986ab5793f299502979cfccf388`.
Its accepted results, two excluded driver-reset attempts and excluded hosted
network attempt retain their recorded classifications. New M2/M3 regression
portrait captures use a separate evidence prefix to preserve accepted images.

## Delivered behavior

Choose a brush in the rail's Retouch selector. Existing procedural tip settings,
pressure/tilt curves, spacing and stabilizers apply. Retouch strength is separate
from stabilizer strength. Flow, opacity and retouch strength accumulate per dab;
ordinary paint retains its existing stroke-opacity cap. Foreground color has no
effect on retouch brushes.

| Tool         | Processing contract                                                                                                                                               |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Smudge       | Move color from the preceding dab's integer-mapped source region toward the current dab. A first stationary dab does nothing. Source alpha weights the color mix. |
| Blur         | Premultiplied 3x3 binomial kernel, unpremultiplied for temporary color mixing. Brush size controls coverage; kernel radius is one document pixel.                 |
| Sharpen      | Add the residual between the current sample and that blur, with channel clamping.                                                                                 |
| Dodge / Burn | Move RGB toward white / black. Tonal ranges weight all samples, shadows `(1-L)^2`, midtones `4L(1-L)` or highlights `L^2`, using sRGB Rec.709-weighted luminance. |
| Sponge       | Desaturate or saturate about that luminance, with clamping.                                                                                                       |

Each dab reads an immutable snapshot of the preceding preview. Results do not
depend on tile/row processing order or cooperative slice size. Current-raster
sampling is intentional; M4 brushes do not sample all visible layers or paint
onto an empty retouch layer. They preserve stored alpha at every target sample,
including exact hidden RGB at zero alpha. Neighbor sampling clamps to the visible
crop and never reads retained outside pixels. Selection coverage clips each dab
once. Pixel/all locks refuse; alpha locks preserve the same alpha contract.
Masks and Quick Mask targets are explicitly refused.

Every stroke publishes one named command, with exact undo/redo and sparse
absence. Escape, focus/capture loss, input overflow, missing hydration or admission
failure discard the candidate and preserve committed pixels, history and redo.
Starting a selection/gradient operation clears stale stroke errors so its own
refusal is visible. No project/recovery schema change is needed.

Shift+G or the Gradient button selects the gradient tool; G remains paint bucket.
Drag a visible start/end line in document coordinates, then release to apply.
Escape, focus loss or changing tools discards the gesture. Ctrl+Z cancels an
unfinished gesture; wheel navigation is disabled during the drag. A zero-length
drag is refused without a mutation. The final fill is a cancellable Rust job and
one Gradient command, with exact undo/redo.

| Shape     | Parameter before clamping to 0..1                                    |
| --------- | -------------------------------------------------------------------- |
| Linear    | Projection along the drag, divided by its length                     |
| Radial    | Distance from the start, divided by drag length                      |
| Angle     | Clockwise full-turn angle relative to the drag direction             |
| Reflected | Absolute linear projection                                           |
| Diamond   | Sum of absolute parallel/perpendicular coordinates in the drag basis |

The editor supports 2..32 strictly ordered RGBA stops, fixed endpoint positions
0/1, adding/removing interior stops, stop movement, opacity and reversal. Mixing
uses premultiplied sRGB temporary colors, so hidden RGB from transparent stops
cannot bleed. Final source-over applies selection once; alpha lock retains target
alpha and hidden RGB. Crop/locks/raster-target rules remain in force.

Black to white, Sunset and Fade to transparent presets ship, along with a
foreground/background shortcut. Save, reuse, update by name and delete custom
presets. Up to 32 presets, 64-character names and 65,536 encoded UTF-8 bytes are
validated before browser storage writes. Corrupt/oversized stored preferences
are ignored; unavailable storage produces a visible error. Presets are UI
preferences, retained across documents/restarts and separate from undo/project
state. M5 brush-preset import/export is not implemented here.

## Bounds and architecture

Pixel processing stays in independent Rust processing crates. The worker owns
documents, retained samples, candidates, history and GPU resources; React/zustand
owns controls and presets. No new dependency is introduced; the approved license
inventory remains 459 expressions.

Stroke work remains bounded by 262,144 destination pixels and at most 512 new dab
attempts per engine step. Blur/sharpen perform nine source samples per affected
pixel. Tip capacity is bounded by the existing 514-square u16 procedural mask;
the input queue remains capped at 2,048 samples. Smudge hydrates two separate
source/destination regions, not the intervening rectangle after skipped
pressure-zero dabs. Only blur/sharpen request a one-pixel halo. Sources remain
immutable until that dab finishes. Bounds are work limits, not a frame-time
guarantee.

Admission reserves coverage, output COW capacity and a possible distinct prior-
dab generation: three RGBA-tile-sized buffers per touched retouch tile, plus
queue/tip capacity. Live residency counts coverage/output reservations and
uniquely retained source generations alongside document/history, paging,
display/composition and GPU allocations. Conservative reservations may exceed
actual sparse usage.

Gradients reuse the bounded fill job: one 256-square tile per step, next-tile
hydration, two RGBA tile scratch allowances and conservative candidate admission.
The bounded decoded-action reservation includes gradient stop/JSON capacity.
Cancellation is checked between cooperative slices and again before the single
commit. A finished candidate can still be cancelled at the final boundary.

Limits remain 768 MiB logical CPU / 128 MiB GPU, 16,384 maximum sides, 128 MiB
encoded images, 4 GiB streamed projects and 2,000,000 gzip WASM bytes. Logical
capacity accounting excludes allocator/map/Arc, browser/driver overhead and
retained WASM allocator pages; these are not process RAM or physical VRAM figures.

## Visual inspection

The unchanged public-domain USAF photograph **Defense.gov News Photo
071212-F-6244S-034**, by Staff Sgt. Eric T. Sheler, depicts Staff Sgt. Melissa Diaz.
Original provenance/license is retained in tests/fixtures/retouch/README.md and
LICENSES.md. The 1905x2889 JPEG is 1,499,675 bytes, SHA-256
`aac5806caab9b8e3d897846ee5dcef29890f14581492a210517439e240fbb014`.

M4 evidence uses the cheek around [1466,1223], a 48 px soft round and a 16 px
horizontal path delivered in eight native moves. Strength is 50% for smudge/blur,
20% for sharpen and 5% for dodge/burn/desaturating sponge. No selection, mask,
crop or lock. Each tool starts from the same exported photograph; exact Undo
restores it. Tests compare all samples outside the bounded neighborhood exactly.

The enlarged crops show directional smearing, slight local smoothing, increased
fine detail contrast, local lightening/darkening and reduced chroma respectively.
They illustrate these tools rather than claiming blemish removal or Poisson
healing quality. Sharpen can amplify noise; smudge/blur soften detail; repeated
tonal/saturation dabs can visibly alter color and clip contrast. The retained
50%-strength hard-tip stress captures deliberately show extreme whitening,
darkening and desaturation. Use lower strength/soft tips for subtle changes.

Gradient shape captures and the RGBA preset test cover orientation, transparency,
reversal, selection boundaries and editor reuse. This does not claim arbitrary
photo-retouch quality or a new Photoshop-reference comparison.

## Hardware measurements

All 44 hardware configurations complete: **176 successful calls**, each configuration's exported before/after SHA-256 matching after four exact Undo restorations. Warm scripted strokes take **498.50–693.74 ms**; full-image gradients take **3.21–6.03 s**. There is no distinct M4 speed target in the roadmap. These are measured limits, with no real-time claim, new speed exception or new deferral.

Windows 10 Pro; Xeon E5-2680 v3 at 2.50 GHz; 34,276,601,856 bytes RAM; NVIDIA GTX 1650/Turing, driver 27.21.14.5751. Chrome 156.0.8078.80 and Edge 156.0.4314.58, non-fallback NVIDIA WebGPU and NVIDIA ANGLE/D3D11 WebGL2, 1280x850 / DPR 1. Serial hardware runs, no Codex builds/tests during timing. This is a shared machine, not a dedicated-host claim.

The photographic fixture is 28,727,686 PNG bytes, SHA-256 `9d3b70d5edb0cdf9939b0bc4cc57f6071128b125744b322e99fbeba577e66478`. Rust crops the unchanged licensed portrait to [0,600,1905,1270], then enlarges with Lanczos3 to 6000x4000 without aspect distortion. This is an enlarged photograph, not a native 24 MP camera capture. The normal populated document is shown at 100%; no diagnostic texture. Current raster, no selection/mask/crop/locks.

Retouch uses a 200 px soft round, flow/opacity 1, spacing 0.1, strength 0.5, mouse pressure 1, stabilizer off. Twelve native Playwright mouse moves cover a 240 px horizontal path with a 20 px sine deviation; the requested wait is 20 ms between moves. Actual warm intermove delivery spans 20.29–27.19 Hz, not an assumed 50/120 Hz stream. Dodge/Burn use All; Sponge desaturates. Gradient controls show Sunset, opacity 0.8; endpoints [2900.5,2000.5]..[3100.5,2000.5], three opaque RGBA stops at 0/0.5/1. Every shape processes the full canvas.

Each browser/API has a fresh context. One first and three warm calls per tool/shape; shared kernels/OS/file caches may already be warm. There is no disk-cold claim. The measured wall interval includes input/automation, worker/cooperative processing, committed presentation and persistence. Opening, control changes, hash exports and Undo are outside timing. Normal autosave stays enabled; outside-timing operations wait for admission. The final run records 0 refused outside-timing RPC admissions, and never retries a measured stroke/gradient.

Values are **milliseconds: first / warm p50 / warm p95**. With three warm calls, p95 and p99 are the maximum observed warm call, not long-term tails.

| Tool      | Chrome WebGPU               | Chrome WebGL2               | Edge WebGPU                 | Edge WebGL2                 |
| --------- | --------------------------- | --------------------------- | --------------------------- | --------------------------- |
| smudge    | 613.87 / 527.54 / 544.87    | 651.68 / 576.36 / 626.91    | 570.87 / 534.88 / 540.48    | 572.63 / 557.60 / 594.02    |
| blur      | 587.21 / 661.73 / 691.83    | 646.23 / 609.38 / 676.97    | 620.32 / 627.45 / 643.79    | 562.14 / 594.73 / 626.96    |
| sharpen   | 646.69 / 626.17 / 644.12    | 621.78 / 661.37 / 676.61    | 681.45 / 678.35 / 693.74    | 630.47 / 658.67 / 659.39    |
| dodge     | 627.59 / 561.22 / 576.41    | 512.59 / 542.78 / 578.01    | 601.32 / 573.68 / 595.86    | 611.69 / 593.50 / 610.59    |
| burn      | 586.65 / 575.14 / 627.66    | 547.04 / 590.39 / 627.54    | 580.13 / 593.59 / 621.13    | 630.51 / 621.30 / 643.36    |
| sponge    | 575.56 / 560.72 / 576.80    | 597.92 / 612.09 / 642.91    | 572.63 / 593.15 / 609.20    | 613.41 / 610.29 / 626.97    |
| linear    | 3566.88 / 3380.55 / 4363.58 | 3512.40 / 3413.96 / 3864.61 | 3383.06 / 3332.77 / 3930.11 | 3154.96 / 3297.13 / 4030.66 |
| radial    | 5782.95 / 5798.13 / 6029.73 | 5615.15 / 5931.38 / 5947.62 | 5703.09 / 5714.04 / 5797.67 | 5584.67 / 5780.49 / 6014.10 |
| angle     | 4814.52 / 4781.36 / 4797.19 | 4682.57 / 4660.54 / 4696.33 | 4793.74 / 4882.79 / 4944.95 | 4829.19 / 4631.02 / 4652.43 |
| reflected | 3460.08 / 3494.79 / 3526.86 | 3526.82 / 3311.69 / 3443.75 | 3366.89 / 3380.77 / 3477.38 | 3419.28 / 3347.95 / 3497.93 |
| diamond   | 3372.39 / 3346.48 / 3412.91 | 3290.04 / 3382.29 / 3581.62 | 3558.61 / 3446.55 / 3613.92 | 3333.24 / 3313.10 / 3346.91 |

Warm retouch acknowledgement and presentation distributions, in milliseconds, aggregate all six tools within each browser/API. They measure observed input-time acknowledgements during active painting. Final completion is measured separately by each complete wall interval above. Unchanged preview frames do not establish distinct pixel-update throughput. Worker frame-paint time includes hydration, processing, composition and persistence; it is not a Rust-kernel-only step time.

| Browser / API   | Changed acknowledgement p50 / p95 / p99 | Completed acknowledgement p50 / p95 / p99 | Presentation gap p50 / p95 / p99 |
| --------------- | --------------------------------------- | ----------------------------------------- | -------------------------------- |
| chrome / WebGPU | 24.43 / 92.93 / 106.82                  | 24.43 / 92.93 / 106.82                    | 19.37 / 67.81 / 96.92            |
| chrome / WebGL2 | 26.30 / 90.47 / 109.50                  | 26.40 / 90.47 / 109.50                    | 23.07 / 72.49 / 101.36           |
| msedge / WebGPU | 24.81 / 87.06 / 102.35                  | 24.81 / 87.06 / 102.35                    | 19.60 / 64.50 / 100.08           |
| msedge / WebGL2 | 29.01 / 86.99 / 105.99                  | 29.01 / 86.99 / 105.99                    | 26.04 / 64.78 / 99.46            |

Observed retouch frame-fence logical CPU peaks reach **102.67 MiB**. These samples are not exhaustive intra-step peaks: the fixed path can touch at most six tiles, whose entire conservative coverage/output/prior-source reservation is at most 4.5 MiB, plus bounded queue/tip capacity and separately admitted paging/display buffers. Gradient worker probes cover cooperative slices, the ready candidate and composition; their CPU peak is **288.51 MiB**, plus at most the separately admitted 2.25 MiB synchronous compositor allowance. Maximum GPU residency is **32.11 MiB**. Admission, conservative reservations and unique generation accounting enforce 768/128 MiB even between observations. These are logical capacities, not process/driver physical memory measurements.

Raw data, per-trial percentiles/input traces, current assets and 44 ordinary-view captures are in `bench/results/p9-m4-performance*`. Reproduce with `pnpm bench:retouch`. Measured processing assets are fingerprinted at `e476b92b2310de0326dddbcd93209e4c68a3dfed`; subsequent report-only commits preserve them. The superseded matrices are retained separately and are not combined with this one.

## Verification and certification

All 220 Rust tests pass, including independent binomial/tonal/gradient numeric
oracles, immutable cumulative smudge, tile/crop/alpha/selection, locks, bounded
cold source/halo requests, disjoint hydration, preallocation refusal, no-op redo,
discarded candidates and exact command undo/redo. Native/WASM clippy and Rust
formatting pass. TypeScript, ESLint, Prettier, eighteen Vitest cases and 459
approved license expressions pass. Windows production WASM is 4,519,376 bytes /
gzip9 1,670,671, below its unchanged 2,000,000-byte gate. Linux deployment bytes
and hashes are recorded separately in final certification.

The clean 64-case focused installed-browser run passes once per case on both
APIs without skips, retries, flaky cases or global errors. Rebuilt WebView2
verifies all six retouch modes and all five gradients, changed pixels/one
command/exact undo/redo, alongside retained clone/healing/content-aware,
format/geometry/recovery checks. Final acceptance additionally requires the
complete 696-case hardware regression on the corrected final runtime, exact-
final-head CI/deployment, the actual downloaded 518-case software report and
both software WebView2 APIs, hosted runtime byte equality and 156 relevant
hosted hardware cases. The immutable certificate records actual completion,
run/artifact identities, counts, hashes, runtime fingerprints and driver events.
No passing subset substitutes for the full matrix.

The first final-head CI candidate failed an existing exact filter-preview
screenshot comparison on D3D11 WARP: one blurred pixel differed between rows of
an otherwise identical three-row fixture. The committed viewport now resolves
nearest atlas samples from fragment screen/document coordinates with integer
texture loads, avoiding interpolated UV ties between its two triangles. The
regression requires identical rows, retains exact preview-to-commit equality and
saves both PNGs. No tolerance or deadline was relaxed. That failed CI run and the
preceding runtime's complete hardware/hosted/timing evidence remain separate;
acceptance uses new complete matrices on the corrected runtime. CI explicitly
uses zero retries and stops a failed matrix promptly; partial matrices never pass
the strict coverage verifier.

Early harness/product investigations are retained in
`bench/results/p9-m4-attempts.json`: initial snapshot readiness, implicit active
layer setup, one zero-test preview-port conflict, stale stroke error visibility,
the superseded partial timing run before disjoint hydration and a later untimed
export/autosave admission collision. None is
declared a final pass. New regression evidence prefixes preserve older P9
portrait artifacts; prior immutable M3 results/classifications are unchanged.

## Deferred work and next milestone

M5 is proposed next: image brush tips, scatter/jitter/texture, CashGear brush
preset import/export, mirror/radial symmetry and phase-wide acceptance. It has
not started. All M5 and phase-wide gates must pass before `p9-retouch`; P10 remains
outside this work.

M1's user-approved all-visible clone 60 fps miss stays deferred to P12. P5's
identifiable Photoshop feather comparison remains open without `p5-select`.
Physical Windows Ink retains its approved deferral. Recorded P6/P7 limits, P8's
approved save-speed deferral and M3 fill-quality limits remain unchanged. M4
introduces no new speed exception and does not silently extend these deferrals.
Stop after M4 acceptance/report before starting M5.
