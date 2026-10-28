# P9 M2 - Healing Brush and Spot Healing

Implementation and focused verification are complete. Final acceptance requires
the full hardware regression, hardware measurements and exact-final-head remote
CI/deployment with inspected artifacts. The milestone completion message and
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

The final measurement matrix and its per-browser/API first/warm timings,
sample-to-GPU-ack latencies, cadence, input path and logical residency are recorded
in `bench/results/p9-m2-performance.json`; run `pnpm bench:healing` to reproduce.
Numerical final results are added after the isolated run. No separate M2 fps/ms
target exists in the original roadmap. No new performance exception is assumed.
Software rendering establishes function only.

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
Full final hardware and remote certification remain mandatory.

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
