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
