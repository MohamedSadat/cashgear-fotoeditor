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

All 44 hardware configurations complete: **176 successful calls**, each configuration's exported before/after SHA-256 matching after four exact Undo restorations. Warm scripted strokes take **478.31–820.61 ms**; full-image gradients take **3.33–6.67 s**. There is no distinct M4 speed target in the roadmap. These are measured limits, with no real-time claim, new speed exception or new deferral.

Windows 10 Pro; Xeon E5-2680 v3 at 2.50 GHz; 34,276,601,856 bytes RAM; NVIDIA GTX 1650/Turing, driver 27.21.14.5751. Chrome 156.0.8078.79 and Edge 156.0.4314.58, non-fallback NVIDIA WebGPU and NVIDIA ANGLE/D3D11 WebGL2, 1280x850 / DPR 1. Serial hardware runs, no Codex builds/tests during timing. This is a shared machine, not a dedicated-host claim.

The photographic fixture is 28,727,686 PNG bytes, SHA-256 `9d3b70d5edb0cdf9939b0bc4cc57f6071128b125744b322e99fbeba577e66478`. Rust crops the unchanged licensed portrait to [0,600,1905,1270], then enlarges with Lanczos3 to 6000x4000 without aspect distortion. This is an enlarged photograph, not a native 24 MP camera capture. The normal populated document is shown at 100%; no diagnostic texture. Current raster, no selection/mask/crop/locks.

Retouch uses a 200 px soft round, flow/opacity 1, spacing 0.1, strength 0.5, mouse pressure 1, stabilizer off. Twelve native Playwright mouse moves cover a 240 px horizontal path with a 20 px sine deviation; the requested wait is 20 ms between moves. Actual warm intermove delivery spans 18.82–28.54 Hz, not an assumed 50/120 Hz stream. Dodge/Burn use All; Sponge desaturates. Gradient controls show Sunset, opacity 0.8; endpoints [2900.5,2000.5]..[3100.5,2000.5], three opaque RGBA stops at 0/0.5/1. Every shape processes the full canvas.

Each browser/API has a fresh context. One first and three warm calls per tool/shape; shared kernels/OS/file caches may already be warm. There is no disk-cold claim. The measured wall interval includes input/automation, worker/cooperative processing, committed presentation and persistence. Opening, control changes, hash exports and Undo are outside timing. Normal autosave stays enabled; outside-timing operations wait for admission. The final run records 0 refused outside-timing RPC admissions, and never retries a measured stroke/gradient.

Values are **milliseconds: first / warm p50 / warm p95**. With three warm calls, p95 and p99 are the maximum observed warm call, not long-term tails.

| Tool      | Chrome WebGPU               | Chrome WebGL2               | Edge WebGPU                 | Edge WebGL2                 |
| --------- | --------------------------- | --------------------------- | --------------------------- | --------------------------- |
| smudge    | 567.87 / 560.88 / 593.99    | 523.27 / 544.35 / 594.52    | 549.14 / 555.98 / 560.30    | 566.65 / 577.89 / 595.21    |
| blur      | 620.12 / 592.64 / 677.97    | 641.47 / 610.88 / 655.87    | 536.00 / 611.78 / 659.24    | 652.96 / 606.83 / 713.73    |
| sharpen   | 642.13 / 624.12 / 624.28    | 606.25 / 641.49 / 677.38    | 632.52 / 592.53 / 628.26    | 613.26 / 796.12 / 820.61    |
| dodge     | 616.95 / 561.82 / 576.59    | 580.06 / 590.69 / 593.20    | 545.01 / 511.35 / 561.83    | 537.73 / 576.99 / 595.00    |
| burn      | 579.98 / 573.93 / 610.92    | 591.45 / 557.29 / 562.77    | 584.17 / 524.60 / 594.51    | 542.03 / 541.52 / 612.13    |
| sponge    | 533.80 / 578.54 / 593.46    | 580.36 / 529.79 / 560.20    | 523.72 / 544.51 / 545.72    | 562.69 / 692.72 / 721.39    |
| linear    | 3325.35 / 3347.69 / 4527.25 | 5015.31 / 3543.00 / 6667.88 | 3609.85 / 3566.95 / 4749.38 | 3755.80 / 4027.35 / 6149.05 |
| radial    | 6026.91 / 5916.24 / 6013.40 | 6180.79 / 6461.60 / 6481.00 | 6320.26 / 6245.94 / 6330.80 | 6206.72 / 6030.01 / 6228.43 |
| angle     | 4953.43 / 5030.37 / 5165.79 | 5282.69 / 5443.74 / 5513.17 | 6510.43 / 6409.60 / 6457.97 | 5264.12 / 5348.84 / 5416.44 |
| reflected | 3645.46 / 3832.17 / 3863.03 | 3709.79 / 3725.44 / 3728.48 | 3768.33 / 3708.98 / 3892.58 | 3800.00 / 3732.35 / 3958.93 |
| diamond   | 3607.50 / 3615.73 / 3731.19 | 3442.86 / 3782.20 / 3849.73 | 3676.91 / 4718.98 / 5253.14 | 3742.01 / 3799.18 / 3929.57 |

Warm retouch acknowledgement and presentation distributions, in milliseconds, aggregate all six tools within each browser/API. They measure observed input-time acknowledgements during active painting. Final completion is measured separately by each complete wall interval above. Unchanged preview frames do not establish distinct pixel-update throughput. Worker frame-paint time includes hydration, processing, composition and persistence; it is not a Rust-kernel-only step time.

| Browser / API   | Changed acknowledgement p50 / p95 / p99 | Completed acknowledgement p50 / p95 / p99 | Presentation gap p50 / p95 / p99 |
| --------------- | --------------------------------------- | ----------------------------------------- | -------------------------------- |
| chrome / WebGPU | 25.57 / 65.69 / 87.74                   | 25.57 / 65.69 / 87.74                     | 19.79 / 53.18 / 78.56            |
| chrome / WebGL2 | 28.11 / 89.92 / 98.11                   | 28.11 / 89.92 / 98.11                     | 19.73 / 66.43 / 101.21           |
| msedge / WebGPU | 26.34 / 74.84 / 82.48                   | 26.34 / 74.84 / 82.48                     | 19.39 / 53.15 / 92.00            |
| msedge / WebGL2 | 30.22 / 116.70 / 136.82                 | 30.22 / 116.70 / 136.82                   | 23.52 / 98.70 / 118.33           |

Observed retouch frame-fence logical CPU peaks reach **102.67 MiB**. These samples are not exhaustive intra-step peaks: the fixed path can touch at most six tiles, whose entire conservative coverage/output/prior-source reservation is at most 4.5 MiB, plus bounded queue/tip capacity and separately admitted paging/display buffers. Gradient worker probes cover cooperative slices, the ready candidate and composition; their CPU peak is **288.51 MiB**, plus at most the separately admitted 2.25 MiB synchronous compositor allowance. Maximum GPU residency is **32.11 MiB**. Admission, conservative reservations and unique generation accounting enforce 768/128 MiB even between observations. These are logical capacities, not process/driver physical memory measurements.

Raw data, per-trial percentiles/input traces, current assets and 44 ordinary-view captures are in `bench/results/p9-m4-performance*`. Reproduce with `pnpm bench:retouch`. Measured processing assets are fingerprinted at `6601524dbae56868a4ae1dc0f2fd48fb71a3ae3b`; subsequent report-only commits preserve them. The superseded matrices are retained separately and are not combined with this one.

## Verification and certification

All 220 Rust tests pass, including independent binomial/tonal/gradient numeric
oracles, immutable cumulative smudge, tile/crop/alpha/selection, locks, bounded
cold source/halo requests, disjoint hydration, preallocation refusal, no-op redo,
discarded candidates and exact command undo/redo. Native/WASM clippy and Rust
formatting pass. TypeScript, ESLint, Prettier, eighteen Vitest cases and 459
approved license expressions pass. Windows production WASM is 4,518,624 bytes /
gzip9 1,670,556, below its unchanged 2,000,000-byte gate. Linux deployment bytes
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
