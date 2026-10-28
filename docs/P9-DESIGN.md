# P9 - Retouch and advanced painting

Started 2026-10-27 directly on clean main at `af66e10` / `p8-files`, without
worktrees or agents. Read AGENTS.md and the complete phase records. Read and
visually inspected the original roadmap's complete P9 section on printed page
17; SHA-256 `83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.

## Product choices and milestones

The user selected clone, then healing/spot healing, then content-aware fill;
CashGear presets only; both mirror and radial symmetry; editable multi-stop
gradient presets. Existing image-tip/texture scope from P2 remains required.

1. M1: Clone Stamp, aligned/non-aligned, current/all visible layers, source
   picking and indicator, existing brush dynamics, selection/locks/crop,
   atomic command/undo/redo/cancellation and bounded source residency.
2. M2: Poisson healing brush and nearby patch-search spot healing. Use an
   identifiable portrait and inspect seam quality; synthetic checks alone do
   not satisfy the roadmap's portrait acceptance.
3. M3: original PatchMatch content-aware fill, bounded/cancellable candidate
   processing, without GPL code.
4. M4: smudge, blur/sharpen, dodge/burn and sponge brushes; linear, radial,
   angle, reflected and diamond gradients with editable multi-stop presets.
5. M5: image tips, scatter/jitter/texture, CashGear preset import/export,
   mirror/radial symmetry, then phase-wide acceptance and `p9-retouch`.

Each milestone is independently verified and reported. Stop at its boundary
before starting the next milestone. M1 does not complete P9. No ABR import.

## M1 behavior and acceptance

- S selects Clone Stamp. Alt-click chooses a source pixel in document
  coordinates. Aligned sampling maintains the initial integer pixel offset
  across strokes; non-aligned sampling starts each stroke at the picked source.
  Changing the source resets alignment. New/open/recovery starts without a source.
- Current-layer sampling reads exact raw straight RGBA samples. All-layer
  sampling reads an immutable pre-stroke visible composite, including enabled
  masks, adjustments/groups/blends/opacity. A stroke never samples its own edits.
  Transparent source pixels are source-over no-ops; foreground color is irrelevant.
  Source and destination are restricted to the visible crop, preserving retained
  outside pixels. Integer source mapping avoids resampling document samples.
- Existing size/hardness/flow/opacity/dynamics/stabilizers apply. Selection
  coverage clips destination writes once. Pixel/all locks refuse the command;
  alpha lock preserves destination alpha and hidden RGB. Mask/Quick Mask targets
  are explicitly refused for Clone Stamp.
- A preview uses COW generations and completes as one named command. Exact
  undo/redo preserves straight alpha, hidden RGB, sparse absence and all metadata.
  Escape/focus loss/input overflow, missing backing, cold-source failure and
  memory refusal never publish the preview or clear redo.
- Count immutable source generations uniquely alongside live document/history,
  candidate coverage, queues, display/cache, paging bridge and GPU residency.
  Hydrate only requested source tiles; release snapshots after commit/cancel.
  Keep 768 MiB logical CPU, 128 MiB GPU, 16,384 sides, 128 MiB images and 4 GiB
  projects; the approved gzip WASM gate stays 2,000,000 bytes.
- Rust tests cover immutable overlap, tile/crop boundaries, alpha/selection,
  locks, admission, exact undo/redo and cold generations. Browser tests exercise
  real source picking/alignment, current/all-layer sampling and cancellation.
- Measure a 200 px clone on the ordinary 6000x4000 document using installed
  Chrome and Edge on actual WebGPU/WebGL2 hardware. Record source mode, brush
  controls, path/input rate, cold/first versus warm conditions, frame throughput,
  p50/p95/p99 cadence/latency and logical CPU/GPU peaks. The source requires 60 fps;
  software CI never establishes that target. Disclose misses rather than silently
  relaxing it or substituting unchanged preview frames for changed-pixel throughput.
  On 2026-10-27 the user explicitly accepted the complete measured M1 limits and
  deferred the all-layer speed target to P12; see DECISIONS.md. The original
  all-layer 60 fps target fails. This exception changes no other gate.
- Run Rust formatting, native/WASM clippy, Cargo tests, TypeScript, ESLint,
  Vitest, production size/licenses/formatting, applicable Chrome/Edge regression
  on both APIs and rebuilt Windows/WebView2 on both APIs. Inspect exact-head
  remote CI/deployment artifacts before reporting M1 accepted. Final phase tag
  additionally requires M2-M5 and the portrait seam gate.

## Inherited limits

P5's identifiable Photoshop feather comparison remains pending without
`p5-select`. Physical Windows Ink has its approved deferral. P6/P7 measured
limits and P8's explicitly accepted save-speed miss (deferred to P12) persist.
P8's thirteen validated raster PSDs do not close the feather gate. P10 is outside
this phase. No new license or dependency is assumed by this design.

## M3 design and acceptance (2026-10-28, before implementation)

Starts on clean main at certified M2 `9f000fd603d6a5394dbc1704d80ae9b377b35766`,
matching fetched origin/main. Re-read the original printed page 17 as extracted
text and a rendered page; the original PDF SHA-256 above matches. Existing
product choices answer the phase questions. Work without agents/worktrees;
stop before M4. M2's immutable certification remains preserved.

- Edit > Content-aware fill requires an explicit nonempty selection and an
  editable raster target. Current layer is the default; all-visible sampling
  supports a separate empty retouch layer. Reuse selection controls' All layers
  option. Mask and Quick Mask targets are refused. Pixel/all locks refuse;
  alpha lock preserves destination alpha and hidden RGB.
- An original Rust multiscale PatchMatch implementation uses deterministic
  initialization, alternating spatial propagation, exponentially shrinking random
  search and patch reconstruction. Conceptual reference: Barnes et al., SIGGRAPH
  2009, https://gfx.cs.princeton.edu/pubs/Barnes_2009_PAR/patchmatch.pdf.
  No reference implementation, GPL code or new processing dependency is used.
- Donors are opaque patches outside every nonzero selection sample, within the
  visible crop and a bounded neighborhood of the selection. Feathered samples
  exclude donors too. Match premultiplied color; hidden RGB never guides search.
  Reject neighborhoods without a suitable patch atomically. This is texture
  synthesis, with no semantic object recognition or guarantee for arbitrary scenes.
- Immutable source and candidate state live in the worker/Rust bridge. Selection
  coverage clips the final source-over write exactly once; all other samples,
  hidden RGB, retained crop pixels, masks, guides, DPI and layer metadata remain
  exact. One Content-aware fill command; byte-exact undo/redo and sparse absence.
- Selection scanning, source capture, pyramid construction, initialization,
  matching, reconstruction and candidate writing use bounded cooperative steps.
  Escape/Cancel, missing/cold backing failures and budget refusal never publish
  partial edits or clear redo. Hydrate only the next requested tiles.
- Admit dense source/mask/pyramid/field/reconstruction buffers before allocation,
  plus COW output tiles, immutable sources, document/history, paging/display/GPU.
  Preserve 768 MiB CPU / 128 MiB GPU, 16,384 sides, 128 MiB images, 4 GiB projects
  and the 2,000,000-byte gzip WASM gate. Report actual bounds and limitations.
- Rust tests cover deterministic nontrivial synthesis, donor exclusion, propagation
  and random search, fractional selection, transparency/hidden RGB, tile/crop
  boundaries, locks, empty/unsuitable sources, cold requests, stepping, budget
  refusal, cancellation/no-op redo and exact command undo/redo. Browser tests use
  actual controls, both sampling modes and cancellation/refusal on both APIs.
- Inspect real-photograph before/after results and magnified crops, retaining
  provenance, hash, selected area and settings. Report visual strengths/limits.
  Measure meaningful 6000x4000 first/warm fills on installed Chrome/Edge hardware
  WebGPU/WebGL2, with source/selection/settings, completion percentiles and logical
  residency. No separate M3 speed threshold or new performance waiver is inferred.
- Run full repository formatting, native/WASM clippy, Cargo tests, TypeScript,
  ESLint, Vitest, production size/licenses, full installed-browser hardware
  regression and rebuilt Windows/WebView2 on both APIs. Inspect exact-final-head
  remote CI/deployment artifacts and hosted bytes/results; require zero failures,
  skips/retries/flaky/global errors. Record immutable certification and an M3
  shipped/deferred/performance/next-M4 report. No `p9-retouch` until M3-M5 pass.

## M2 design and acceptance (2026-10-28, before implementation)

Starts at certified M1 `c630690977e1a3ca3a5558f2dd0d3ee78808540f`, with a
clean checkout matching fetched origin/main. Re-read the complete printed page
17 visually and verify the original PDF SHA-256 above. All product questions
are answered. Work directly on main, without agents/worktrees; stop before M3.

- Healing Brush: Alt-click source, aligned/non-aligned integer mapping and
  current/all-visible sampling reuse M1. Spot Healing needs no source and searches
  nearby non-overlapping patches, scoring the surrounding ring rather than the
  blemish interior. Deterministic bounded coarse search/refinement is local patch
  search, not M3 PatchMatch or content-aware fill.
- Solve the discrete source-gradient Poisson equation with immutable destination
  Dirichlet boundary values on each dab's shaped domain. An original conjugate-
  gradient implementation yields between bounded stages/rows; no external code
  or new dependency. Reference: Perez, Gangnet and Blake, SIGGRAPH 2003,
  https://legacy.sites.fas.harvard.edu/~cs278/papers/poisson.pdf.
- Apply the solved RGB through existing stroke coverage/opacity/flow and selection
  exactly once. Transparent sampled source/target pixels have no healing domain;
  hidden RGB cannot guide a solve. All-visible sampling supports a separate empty
  retouch layer using the immutable visible target as its boundary. Preserve
  source-over alpha behavior and alpha lock. Mask/Quick Mask targets are refused.
- Immutable pre-stroke samples are used throughout overlap and search. Source
  and target domains and their one-pixel halo respect the visible crop. Retained
  samples, guides/DPI, legacy projects/recovery and unrelated RGB stay unchanged.
- One named command per stroke, exact undo/redo. Escape/focus loss/overflow,
  failed hydration, unsuitable source, solver failure and admission refusal must
  preserve committed pixels, history and redo. Candidate previews may be discarded.
- Count search, solver vectors/domain/sample buffers, coverage, output COW tiles,
  immutable source generations, history/paging/display and GPU residency. Keep
  768 MiB CPU / 128 MiB GPU, existing dimension/file caps and 2,000,000-byte gzip
  WASM gate. Hydrate only bounded next-dab source/target/search regions.
- Rust verification: independent small-domain Poisson oracle, nontrivial texture
  and tonal adaptation, deterministic search excluding the blemish, immutable
  overlap, tile/crop/alpha/selection/locks, bounded stepping/admission/cold pages,
  cancellation/refusal/no-op redo and exact undo/redo. Browser verification uses
  real controls/source picking and worker jobs on both installed browsers/APIs.
- Portrait gate: acquire an identifiable licensed reference portrait with a real
  visible blemish; retain provenance/hash/coordinates/settings and inspect full
  before/after and magnified crops for both tools. Record blemish removal and seam
  assessment explicitly; synthetic-only evidence cannot pass this gate.
- Measure first/warm 6000x4000 M2 strokes on verified Chrome/Edge hardware APIs:
  settings, source mode, input path, completion/cadence/latency percentiles and CPU/
  GPU peaks. No separate M2 fps/ms threshold exists in the roadmap; no new speed
  exception or under-one-frame claim is assumed. Software proves function only.
- Run full formatting, native/WASM clippy, Cargo tests, TypeScript, ESLint,
  Vitest, production size/licenses, required hardware regression and rebuilt
  WebView2 on both APIs. Inspect exact-final-head CI/deployment and downloaded
  artifacts for zero failures/skips/retries/flaky/global errors before acceptance.
  Report shipped/deferred work, portrait evidence, measured performance, validation
  and proposed M3, then stop. No phase tag until all P9 milestones pass.
