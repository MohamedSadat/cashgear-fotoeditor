# P6 adjustments and filters

The source is the complete P6 section on printed pages 14–15 of the original
roadmap. User choices and sequencing are in DECISIONS.md. Implement directly on
main with small verified commits, without agents or a worktree.

## Milestones

M1 ships the first five destructive adjustments and an always-available histogram
panel. M2 adds adjustment-layer metadata/commands, streaming persistence, shared
CPU/GPU adjustment math, and viewport preview. M3 ships the five priority filters,
their live previews and Fade Last Filter. M4 completes the remaining seven
adjustments and five filters, then validates the whole phase. PLAN.md lists every
source feature and phase-wide performance target. No milestone substitutes for
the final phase gates or authorizes a P6 completion tag early.

The existing GPU compositor produces a full-resolution flattened display cache.
Fast per-layer preview therefore needs its own compositor milestone. M1 controls
explicitly apply on confirmation; slider changes do not preview or mutate pixels.
This interim behavior is intentional and must be reported as deferred P6 work.

## M1 numerical contract

All algorithms and document pixel loops live in fotoeditor-filters, depending
only on browser-independent core and already-approved serde. React owns controls;
the worker owns immutable input snapshots, candidates, histogram jobs and tile
hydration. No JavaScript document-pixel loops or arrays.

Inputs and outputs are straight RGBA8. Color adjustments preserve alpha exactly
and leave fully transparent hidden RGB unchanged. Zero selection coverage returns
the exact original; fractional selection coverage interpolates original/adjusted
straight RGB once with nearest rounding. Alpha-lock does not forbid color edits;
pixel/all/ancestor locks do. M1 targets raster pixels; masks/Quick Mask/groups give
a visible request to select a raster pixel target.

These are FotoEditor-defined, deterministic sRGB controls, not a claim of Photoshop
formula equivalence. Values are clamped to [0,1] and quantized once at output:

- Brightness adds [-1,1] in sRGB; contrast multiplies around 0.5 by 2^(2c), c in
  [-1,1]. Brightness is applied before contrast.
- Levels normalizes input black/white, raises to 1/gamma, then maps output
  black/white. Input black must be less than white; gamma is [0.1,10].
- Curves uses a master RGB piecewise-linear curve with 2–16 strictly increasing
  x control points, fixed x endpoints 0/1, y values in [0,1].
- Exposure decodes the sRGB transfer function, multiplies linear light by
  2^stops ([-10,10]), adds linear offset [-1,1], applies 1/gamma, then encodes sRGB.
- Hue/saturation uses HSL: hue rotates ±180 degrees, saturation scales by 1+s
  (s in [-1,1]), lightness moves toward white or black by l in [-1,1].

Invalid/non-finite settings are rejected before job allocation. Identity settings
return exact samples. Separable channel controls prepare 256-entry LUTs; HSL is
evaluated on straight RGB. The same specification is the M2 parity contract.

## M1 jobs and histogram

Adjustment jobs inspect one existing raster tile per step. Empty transparent
tiles stay absent; boundary padding is unchanged. Inputs and selection coverage
are pinned before sampling. The worker hydrates only the next tile, yields after
approximately four milliseconds, and checks cancellation before atomic command
commit. Incomplete, cancelled, invalid and budget-refused jobs preserve document,
history and redo. COW candidate generations and bounded scratch are counted once
against the shared CPU limit and enumerated for paging. Final application is one
named command. Project/recovery formats remain unchanged because M1 edits samples.

The exact full active-raster histogram reports 256 RGB and sRGB-luma bins, weighted
by stored alpha (0–255) to exclude hidden RGB and avoid opaque weighting of faint
samples. It ignores selection, layer opacity/masks/blend and visibility; the panel
names this source. Each nontransparent source pixel contributes alpha weight to
each channel; totals are integer exact, not a viewport sample. Background refresh
uses immutable source handles, bounded tile steps and paging. Stale results must
not replace newer content/active-layer results. A read-only histogram creates no
history entry and does not mark recovery dirty.

## M1 acceptance

- Independent numerical vectors and LUT/reference comparisons for every control;
  identity, alpha 0/1/128/255, partial coverage and hidden RGB.
- Exact undo/redo, no-op/redo preservation, locks, invalid settings, cancelled and
  over-budget candidates, odd dimensions/tile seams and cold paged inputs.
- Exact histogram bins/alpha totals; sparse blank/hidden samples and no mutation.
- Browser controls and exported pixels on installed Chrome/Edge, both GPU APIs;
  project/recovery sample persistence and Windows/WebView2 integration.
- Required Rust/JS checks, license/size gates, measured hardware apply/histogram
  timings and residency, green remote CI/deployment; milestone report and stop
  before M2. No blur or slider-preview performance claim is valid in M1.

The P5 Photoshop feather comparison remains pending and is not waived by P6.

## M2 implementation contract

The user resumed M2 on 2026-10-24. Adjustment settings become validated layer
content with command-based insertion/editing, exact metadata undo, duplication,
reordering and masks. The core crate owns the shared numerical definitions so
the CPU compositor can evaluate them without introducing a dependency cycle;
the filters crate continues to expose the same adjustment API for destructive
jobs. Adjustment steps transform the straight color of the accumulated backdrop
and preserve its alpha, interpolating by mask, opacity and fill, with the layer's
blend function. Empty backdrops remain empty. Group isolation and clipping use
the existing ordered compositor plan.

Selection on insertion becomes the adjustment layer's editable mask. Sparse mask
defaults must survive composition, painting, commands and streaming persistence.
New manifests identify their schema; existing raster/group projects remain
readable. Corrupt settings or unsupported schema versions reject the candidate
before replacing the live document.

Viewport preview is temporary worker/GPU state, with no history or recovery
mutation. Preparing its bounded visible source cache may require paging; report
that preparation separately from warm control-change latency. Slider updates
must use the GPU, coalesce stale requests and preserve the full layer tree's
ordering/masks/blends. Applying still uses the full-resolution selected area or
commits adjustment metadata. Cancellation, document changes and worker replacement
must drop temporary preview state. CPU/GPU budgets include preview sources and
render targets. M2 acceptance requires measured warm preview latency below
100 ms plus independent CPU/GPU and destructive/layer comparisons within one
channel value, project/recovery integration and all applicable repository gates.
