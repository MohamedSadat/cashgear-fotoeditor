# P9 M3 - Content-aware fill

M3 delivers selection-based content-aware fill with an original Rust PatchMatch
implementation. Completion is certified by
`bench/results/p9-m3-final-certification.json` after the full final-head local,
remote and hosted gates pass. This milestone does not complete P9, and does not
authorize `p9-retouch` or M4 implementation.

Started 2026-10-28 directly on clean main at certified M2
`9f000fd603d6a5394dbc1704d80ae9b377b35766`, matching fetched origin/main, without
agents or worktrees. Read AGENTS.md, PLAN.md, PROGRESS.md, DECISIONS.md,
P9-DESIGN.md, P9-M2-REPORT.md and M2's immutable certification. Read the complete
original P9 section on printed page 17 as text and a rendered page; the source
PDF SHA-256 remains
`83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.
Existing product choices answer the phase questions. M3 acceptance was recorded
in P9-DESIGN.md before implementation.

## Delivered behavior

- Select an area, then Edit > Content-aware fill. The selection controls' All
  layers checkbox chooses an immutable visible composite; unchecked uses exact
  current-raster samples. All-visible sampling supports an empty retouch layer
  above the image. Foreground/background colors are irrelevant.
- The original multiscale Rust implementation builds a masked source pyramid,
  initializes from nearby valid donors, then alternates forward/backward spatial
  propagation and exponentially shrinking random search. Each matching pass also
  tries a uniformly sampled valid donor. Weighted patch votes reconstruct the
  hole; the coarser result initializes the next finer scale. The fixed seed makes
  repeated fills of identical input reproducible.
- Patches are 7x7, with six matching/reconstruction iterations at each of at most
  six scales. The selected bounding box is extended by its largest side, clamped
  to 128..512 pixels and the visible crop. Every donor patch is fully opaque and
  excludes all nonzero selection coverage, including feathered edges. Selected
  object colors cannot seed the coarse pyramid or guide synthesis.
- Matching uses premultiplied target color and ignores zero-alpha RGB. Final
  source-over writes use selection coverage exactly once. Pixel/all locks refuse;
  alpha lock preserves destination alpha and transparent hidden RGB. Mask and
  Quick Mask targets are explicitly refused. Empty selections and neighborhoods
  without a suitable donor produce a visible refusal.
- One Content-aware fill command; exact undo/redo, including sparse absence.
  Immutable sources, masks, selection, retained crop samples, guides, DPI and
  unrelated hidden RGB retain their existing contracts. No project format or
  recovery schema change is needed.
- Cancel operation or Escape discards an unfinished candidate. Cancellation,
  missing hydration and memory refusal preserve committed pixels/history/redo.
  A ready-to-commit test covers the final cancellation boundary; another test
  cancels actual processing. Once the command commits, subsequent composition
  finishes and cancellation is no longer accepted, matching existing job behavior.

Pixel processing stays in the browser/UI/GPU-independent processing crates. The
worker owns document/history/source/candidate/GPU state; React/zustand owns the
controls. The conceptual reference is [Barnes et al., SIGGRAPH
2009](https://gfx.cs.princeton.edu/pubs/Barnes_2009_PAR/patchmatch.pdf). No reference
implementation, GPL code or new runtime dependency is used. The approved license
inventory remains 459 expressions. The unchanged public-domain portrait's existing
provenance/license record remains in tests/fixtures/retouch/README.md and LICENSES.md.

## Bounds and limitations

Selection scanning and candidate writing handle one 256-square tile per step;
capture handles one tile-row segment. Pyramid/nearest/seed stages handle at most
2048 pixels, donor validation 512, matching 32 positions and reconstruction 256.
Each matching position tries two propagated offsets, one random donor and a
finite halving-radius random search. The worker yields between cooperative slices.
These are work bounds, not a guarantee that every step finishes within one frame.

Dense capacities are admitted before allocation: per pyramid pixel, four RGBA
bytes for source, one mask byte, two four-byte reconstruction surfaces, an eight-
byte match, one validity byte and four bytes of donor-index capacity total 26
bytes. Add two RGBA tile scratch buffers and a conservative bounding-box allowance
for every COW output tile. Count these alongside uniquely retained source,
document/history, paging, display/composition and GPU resources. Scratch/source
state is released on finish/cancel/refusal. Coarse-level fallback retains its
conservative reservation even when an unsuitable level is discarded.

Limits remain 768 MiB logical CPU / 128 MiB GPU, 16,384 maximum sides, 128 MiB
encoded images, 4 GiB streamed projects and 2,000,000 gzip WASM bytes. Large areas
can be refused before allocation. Logical capacities exclude allocator/map/Arc,
browser/driver overhead and retained WASM allocator pages; they are not process
RAM or VRAM measurements.

This is local texture synthesis. Donors must fit outside the selection within
the automatic neighborhood. There is no semantic object recognition, editable
donor mask or arbitrary-scene quality guarantee. Patch voting can soften texture,
repeat structures or leave visible changes in difficult gradients/lines. The
200-square synthetic diagonal-pattern benchmark visibly shows this limitation;
its processing timings are not evidence of seamless reconstruction. The portrait
example below is a bounded successful real-image case.

## Real photograph inspection

The unchanged **Defense.gov News Photo 071212-F-6244S-034** depicts Staff Sgt.
Melissa Diaz, photographed by Staff Sgt. Eric T. Sheler, U.S. Air Force, on
12 December 2007. The public-domain federal photograph is identified at
https://commons.wikimedia.org/wiki/File:Defense.gov_News_Photo_071212-F-6244S-034.jpg.
Its 1905x2889 JPEG is 1,499,675 bytes, SHA-256
`aac5806caab9b8e3d897846ee5dcef29890f14581492a210517439e240fbb014`.

The actual cheek blemish at [1466,1223] is enclosed by an anti-aliased ellipse
from [1450,1205] to [1483,1242]. Current-raster sampling, default deterministic
settings, no masks/crop/locks. No blemish is injected. Full before/after exports,
ordinary 100% app captures and the enlarged 2x neighborhood were inspected. The
blemish is removed without an obvious ellipse boundary; the synthesized area is
slightly softer than neighboring skin texture. This is an assessment of this
selection on this portrait, not a general retouch-quality guarantee.

Browser checks compare every sample outside the selection's bounding rectangle
byte-for-byte, verify the center changes and redness falls, then require an exact
export after Undo. Rust tests independently cover fractional coverage, untouched
samples and exclusion of selected source colors at multiple scales.
Evidence uses `bench/results/p9-m3-portrait-*`, `p9-m3-portrait-detail.png` and the
portrait record in the final certification. M2's accepted Poisson portrait evidence
is preserved separately and is not replaced by this example.

## Hardware measurements

All sixteen hardware configurations complete: 64 fills. Warm current-raster
48-square fills take 272.75-358.32 ms; 200-square fills take 2.94405-3.46494 s.
All-visible fills into a separate retouch layer take 5.39473-7.52790 s and
8.31041-11.29218 s respectively. Full-document composition after commit dominates
the additional all-visible time. Large-area and all-visible latency remain
limitations. The roadmap specifies no separate M3 speed threshold; no new speed
exception or real-time claim is inferred.

Windows 10 Pro 10.0.19045; Xeon E5-2680 v3 at 2.50 GHz; 34,276,601,856 bytes RAM;
NVIDIA GTX 1650/Turing, driver 27.21.14.5751. Chrome 156.0.8078.49 and
Edge 156.0.4314.42, serial non-fallback NVIDIA WebGPU and NVIDIA ANGLE/D3D11
WebGL2, 1280x850 / DPR 1. No Codex builds/tests run during timing. This is a shared
machine, not a dedicated-host claim.

The ordinary dense 6000x4000 RGBA8 fixture is 67,038,649 bytes, SHA-256
`809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5`.
Normal populated document view at 100%, no diagnostic texture. Centered hard
rectangular selections of 48x48 and 200x200 pixels. Current mode edits the image
raster; all mode edits a separate empty raster above it. Six iterations per scale,
7x7 patches, automatic neighborhood, no masks/crop/locks.

Each browser/API/source mode uses a fresh context. Four calls per size comprise
the first call with these controls and three warm calls. Shared kernels may
already be warm from the preceding size. OS/file caches are not cleared; there
is no disk-cold claim. Exact Undo outside timing restores the input between calls;
the exported SHA-256 before and after all four calls matches in every configuration.
Times include API/automation, worker queuing/cooperative CPU, composition, GPU
acknowledgement and persistence until committed presentation is observed. They
exclude prior opening, selection creation, exports/hashing and Undo.

Values are milliseconds: **first / warm p50 / p95 / p99**. With only three warm
calls, empirical p95/p99 are the maximum observed warm call, not long-term tails.

| Browser / API   | Source  | 48x48                                 | 200x200                                   |
| --------------- | ------- | ------------------------------------- | ----------------------------------------- |
| Chrome / WebGPU | current | 285.27 / 310.03 / 342.98 / 342.98     | 3269.70 / 3370.32 / 3464.94 / 3464.94     |
| Chrome / WebGPU | all     | 5620.80 / 5510.76 / 5671.26 / 5671.26 | 8445.62 / 8745.13 / 8826.80 / 8826.80     |
| Chrome / WebGL2 | current | 370.46 / 326.76 / 358.32 / 358.32     | 3154.02 / 3392.67 / 3464.38 / 3464.38     |
| Chrome / WebGL2 | all     | 7375.70 / 7360.90 / 7527.90 / 7527.90 | 10414.38 / 10427.33 / 11292.18 / 11292.18 |
| Edge / WebGPU   | current | 293.63 / 293.21 / 325.18 / 325.18     | 3177.59 / 3078.24 / 3260.94 / 3260.94     |
| Edge / WebGPU   | all     | 6043.73 / 5741.46 / 6511.03 / 6511.03 | 9325.77 / 8643.20 / 9024.18 / 9024.18     |
| Edge / WebGL2   | current | 345.21 / 279.07 / 328.40 / 328.40     | 3130.76 / 3042.56 / 3209.30 / 3209.30     |
| Edge / WebGL2   | all     | 7154.04 / 7076.21 / 7124.97 / 7124.97 | 10050.34 / 10292.29 / 10358.80 / 10358.80 |

Kernel totals span 187.01-259.50 ms at 48-square and 2.47501-3.19027 s at
200-square. These exclude worker yields, memory probes and composition. The
largest measured selection-step call is 22.38 ms; it includes engine admission/
command work and possible JIT/scheduling. No under-one-frame guarantee is made.
The fill publishes atomically: presented-frame gap/cadence records in the raw data
are waiting intervals between committed frames, not a continuous preview fps rate.

Worker probes read logical residency at cooperative slices, before commit and
across composition tiles/readback. Observed maxima are 111.46 MiB CPU / 32.90 MiB
GPU in current mode and 293.85 / 21.17 MiB in all mode. The separately admitted
synchronous compositor reference/output allowance can add at most 2.25 MiB beyond
a probe, remaining below 768 / 128 MiB. Temporary reconstruction capacity is
reserved, not inferred from physical RAM. End-frame observers alone missed the
temporary composite and are not used as the final job peak measurement.

Raw evidence is `bench/results/p9-m3-performance.json`, its summary JSON,
runtime asset SHA-256s and sixteen ordinary-view captures. The benchmark contact
sheet was inspected. Reproduce with `pnpm bench:content-aware`. Measured runtime
is `7fa6ebb573e1b6deba02af48614aa8864acabb7e`; later documentation changes must
preserve these processing assets.

## Verification and certification

All 207 Rust tests pass, including eleven new content-aware cases: an independent
patch-distance oracle, nontrivial deterministic synthesis, donor exclusion across
scales, tile/crop edges, exact coverage/hidden RGB/alpha lock, empty retouch layers,
locks, unsuitable/empty sources, preallocation refusal, cold next-tile requests,
discarded/no-op candidates, redo preservation and exact command undo/redo.
Native/WASM clippy, Rust formatting, TypeScript, ESLint, Prettier, sixteen Vitest
tests and 459 approved license expressions pass. Local production WASM is
4,494,742 bytes / gzip9 1,662,210 within the unchanged 2,000,000-byte gate.

Fifteen final-worker focused software cases pass once each with no skips/retries/
flaky/global errors; these prove function only. Rebuilt WebView2 156.0.4314.50
passes both actual NVIDIA APIs, including content-aware pixels/one command/exact
undo/redo plus the retained clone/healing/format/geometry/recovery checks.

Final acceptance gates are the complete 632-case installed Chrome/Edge hardware
matrix on both APIs; exact-final-head remote CI/deployment; downloaded CI browser,
desktop, Rust/Vitest/size/license results; and hosted hardware/asset verification.
Every browser case must have exactly one passing attempt with zero skips/retries/
flaky/global errors. The final certification records actual reports/counts,
immutable run/artifact identities, hashes and completion status. Evidence paths
are `p9-m3-final-browser*`, `p9-m3-probes-desktop*`, `p9-m3-software-focused*`,
`p9-m3-hosted-*` and the final-head CI/deployment directories under bench/results.
All eight hosted runtime files must match the preserved deployment artifact;
local Windows and deployment Linux builds are fingerprinted separately.

Early harness investigations are preserved separately: the all-visible fixture
initially inserted its layer below the image; a cancellation assertion compared
the whole status bar without its Cancel button text. These are corrected with
unchanged pixel/history assertions. An initial reporter CLI failed before tests.
The partial end-frame-only timing run was intentionally stopped and superseded by
the complete worker-probed matrix. None is substituted for clean final evidence.

## Deferred work and next milestone

M4 is proposed next: smudge, blur/sharpen, dodge/burn and sponge brushes, plus
linear/radial/angle/reflected/diamond gradients and editable multi-stop presets.
It has not started. M5's image tips/scatter/jitter/texture/CashGear presets and
mirror/radial symmetry, followed by phase-wide acceptance, still gate `p9-retouch`.
P10 remains outside scope.

M1's user-approved all-visible clone 60 fps miss stays deferred to P12. P5's
identifiable Photoshop feather comparison remains open without `p5-select`.
Physical Windows Ink keeps its approved deferral. Recorded P6/P7 limits and P8's
accepted save-speed deferral remain unchanged. No new speed exception is requested
or inferred. Stop after M3 certification before starting M4.
