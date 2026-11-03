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

## M5 design and acceptance (2026-11-03, before implementation)

The user explicitly authorizes M5 and final P9 acceptance, directly on main,
without agents/worktrees. Clean main and fetched origin/main both start at
accepted M4 `7d4f28313f83ad66faab5b67df1e515c4e7d7050`. Read the required phase
records and M4 report. Re-read the complete original printed page 17 as extracted
text and a newly rendered page; its recorded SHA-256 matches. M4 certification
SHA-256 remains `69b3efaee85ebb25ea6ce90a48c83988bec3f41e15b60113700a144b6227e66f`;
M3 certification and accepted artifacts/classifications remain immutable too.
Existing product decisions answer all questions. The following engineering
defaults make the remaining roadmap behavior testable before coding.

- Load image tips and textures from 8-bit PNG, JPEG or WebP, up to 512 pixels per
  side and 4 MiB encoded. Check dimensions and decoder allocations before decode.
  Rust converts to coverage: inverse sRGB luminance times alpha; black paints,
  white/transparent leaves unchanged, and hidden RGB has no effect. Preserve
  aspect ratio, bilinear tip sampling and existing roundness/angle/dynamics.
  Image size denotes the circumscribed diameter so rotated corners are retained.
  Procedural round tips remain the default. Pencil uses its existing exact pixel.
- Add bounded scatter (0..2 brush diameters), size/angle/opacity jitter (0..1),
  a user-editable u32 seed and repeatable per-dab Rust PRNG. Randomness advances
  once per logical dab, independent of cooperative slicing/hydration. Size jitter
  reduces size; opacity jitter reduces influence; angle jitter spans +/-180
  degrees at maximum. Scatter offsets each dab in a disk. Texture repeats in
  dab-local coordinates, with 1..512 document-pixel period and 0..1 depth;
  multiply coverage once before flow/selection. Settings apply to brush, eraser
  and the existing retouch tools; retouch sampling contracts stay unchanged.
- Mirror painting offers vertical, horizontal and both axes; radial painting
  offers 2..16 sectors. The editable center is in retained document coordinates,
  defaulting to the visible canvas center. Show axes/spokes in the viewport,
  correctly following pan/zoom/rotation/flip/crop. Paint/eraser, including mask
  and Quick Mask painting, replicate each jittered dab and its oriented image
  shape in Rust. Coincident copies are deduplicated. Retouch tools retain their
  explicitly displayed single-path sampling behavior; symmetry is a painting
  option, not a change to clone/healing source mapping. One stroke is one command.
- Named CashGear presets capture tip/texture, brush shape/dynamics, scatter/jitter
  and seed; foreground color, active tool and document-relative symmetry center
  remain separate controls. Save/update/delete/reuse across documents/restarts.
  Self-contained version-1 `.cgbrush` JSON imports/exports a validated library,
  bounded to 32 presets / 4 MiB UTF-8, 64-character unique names and canonical
  bounded coverage assets. No ABR. Import validates the whole candidate before
  replacing presets; malformed/future/oversized files and storage failures leave
  existing presets/settings/document/history/redo intact with a visible error.
- All writes use the existing Rust command path, preserving selection coverage
  exactly once, alpha locks/hidden RGB, sparse absence and exact undo/redo. Preserve
  masks, crop-retained samples, guides/DPI, paging, project/recovery and legacy
  reading. Escape/focus loss, input overflow, hydration failure and budget refusal
  discard the whole candidate. Count decoded settings/assets, prepared tip/texture,
  bounded symmetry queue, coverage/output/history/source/paging/display/GPU memory.
  Keep 768 MiB logical CPU / 128 MiB GPU, 16,384 sides, 128 MiB document images,
  4 GiB projects and 2,000,000-byte gzip WASM. No new dependency is planned.
- Rust tests require independent coverage/transform oracles, deterministic results
  across step sizes, transformed asymmetric tips, texture/scatter/jitter bounds,
  coincident-copy handling, tile/crop/selection/alpha/locks, cold pages, bounded
  memory/admission, cancellation/no-op redo and exact undo/redo. UI unit/browser
  tests cover full preset roundtrip/refusal, actual import/settings/symmetry
  controls, masks/Quick Mask and persistence. Inspect ordinary-view and enlarged
  image-tip/texture/symmetry captures on both hardware APIs.
- Measure meaningful first/warm 6000x4000 M5 strokes on verified installed
  Chrome/Edge WebGPU/WebGL2, with dataset, settings/seed/path/input delivery,
  completion and acknowledgement percentiles, logical CPU/GPU bounds and runtime
  fingerprints. Repeat phase clone measurements and retained portrait checks.
  No separate M5 fps/ms target exists; report limits. M1's accepted all-visible
  60 fps miss remains a P12 deferral, never a passed original target.
- Complete formatting, native/WASM clippy, Cargo tests, TS/ESLint/Prettier/Vitest,
  production build/size/licenses, a fresh complete hardware browser matrix and
  rebuilt Windows/WebView2 on both APIs. Require exact-final-head CI/deployment,
  actual downloaded software/browser/desktop artifacts, hosted byte equality and
  relevant hosted hardware tests: zero failures/skips/retries/flaky/global errors.
  Keep failed/superseded attempts separate; no union of passing subsets. Preserve
  accepted M3/M4 evidence by using new prefixes for every regression artifact.
- Only after all remaining P9 gates pass, write immutable final certification and
  a P9 shipped/deferred/performance report, create/push `p9-retouch`, propose P10
  and stop. P5's open feather comparison, physical Ink deferral, P6/P7/P8 limits,
  M3 quality limits and existing user-approved P12 deferrals retain their status.

## M4 design and acceptance (2026-11-02, before implementation)

Starts at certified M3 `2ab9dbdd6820b23196cb1c5b2f1f4b0baab13d11`, clean main
matching fetched origin/main. Re-read the complete original printed page 17 as
extracted text and a newly rendered page; its SHA-256 above matches. Preserved
M3 certification SHA-256 is
`60c6b6789622908e6e82d9a5ed889bec9f1c1986ab5793f299502979cfccf388`.
The user explicitly authorized M4 without agents/worktrees. Existing product
choices resolve all questions; stop before M5 and do not create `p9-retouch`.

- Add Smudge, Blur, Sharpen, Dodge, Burn and Sponge tools. Reuse procedural tip,
  size/hardness/roundness/angle, spacing, flow/opacity, pressure/tilt curves and
  stabilizers. Separate retouch strength controls cumulative per-dab mixing.
  Sample the current raster only. Foreground color has no effect. Mask/Quick
  Mask targets are explicitly refused; pixel/all locks refuse; alpha is retained,
  including exact hidden RGB at zero alpha. Selection clips each dab once.
- Smudge advects the preceding dab's raster toward the new dab, mixed by strength;
  a stationary first dab does nothing. Blur uses a premultiplied 3x3 binomial
  kernel; Sharpen adds its high-frequency residual with channel clamping. Each
  dab reads an immutable snapshot of the preceding preview, so tile order cannot
  affect the result. Crop-clamped neighbors never sample retained outside pixels.
  Dodge/Burn apply exposure toward white/black with selectable shadows/midtones/
  highlights/all tonal weighting. Sponge saturates/desaturates about luminance.
  Cumulative dabs intentionally differ from ordinary paint's stroke-opacity cap.
- One named command per stroke, exact undo/redo and sparse absence. Escape,
  focus loss, overflow, missing backing and budget refusal discard the candidate,
  preserving committed pixels/history/redo. Hydrate only next-dab source/halo and
  destination regions. Count retained source generations and COW output/coverage,
  input/tip buffers, paging, document/history/display and GPU residency.
- Gradient drag defines start/end in document coordinates, with a visible line
  before release; Escape/focus loss cancels the gesture. Five shapes: projected
  linear, distance radial, oriented full-turn angle, absolute projected reflected,
  and oriented Manhattan-distance diamond. Clamp to endpoint colors. A zero-length
  drag is refused without mutation. Editor supports 2..32 strictly ordered stops,
  endpoint stops at 0/1, RGB and alpha, opacity and reversal. Interpolate sRGB in
  premultiplied temporary color; never let transparent hidden RGB bleed. Apply
  source-over through selection once; alpha lock preserves target alpha/hidden RGB.
  Gradient targets raster pixels; explicitly refuse masks/Quick Mask.
- Ship built-in gradients and editable named reusable multi-stop presets, persisted
  as bounded validated local UI preferences. Presets do not mutate the document.
  New/open/recovery retain UI presets; document/project formats stay unchanged.
  M5 brush-preset import/export remains out of scope. Gradient fills run in bounded
  Rust tile steps, yielding for cancellation and hydration, one Gradient command.
- Verify independent numeric kernel/shape/interpolation oracles, cumulative smear,
  tonal/saturation direction, tile/crop/selection/alpha/locks, cold-source requests,
  admission/refusal/cancel/no-op redo and exact undo/redo. Browser cases use actual
  tool/strength/gradient editor/preset/drag controls on both installed browsers/APIs.
  Inspect realistic photo before/after and gradient transparency/shape captures.
- Measure first/warm 6000x4000 retouch strokes and gradients on verified hardware
  Chrome/Edge WebGPU/WebGL2, recording controls/path, latency/completion/cadence,
  logical CPU/GPU peaks and runtime fingerprints. The roadmap has no distinct M4
  speed target; disclose observed limits without adding an exception or fps claim.
- Preserve 768 MiB CPU / 128 MiB GPU, 16384 sides, 128 MiB encoded images, 4 GiB
  projects and 2,000,000-byte gzip WASM. No new dependency is planned. Run formatting,
  native/WASM clippy, Cargo tests, TS/ESLint/Prettier/Vitest/licenses/build/size,
  full installed-browser hardware regression and rebuilt WebView2 on both APIs.
  Require exact-final-head CI/deployment, downloaded artifact inspection, hosted
  byte equality and relevant hosted hardware tests with zero failures, skips,
  retries, flaky cases or global errors. Only then write immutable certification
  and shipped/deferred/performance/next-M5 report. Inherited limits remain unchanged.

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
