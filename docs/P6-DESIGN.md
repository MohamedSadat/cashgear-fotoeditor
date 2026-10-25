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
the existing ordered compositor plan. When an adjustment is the base of a clipping
chain, the chain inherits the accumulated backdrop and retains its alpha. The
adjustment's own blend function is evaluated against that inherited backdrop.

An adjustment step quantizes its inherited straight color to bytes before LUT/
HSL evaluation and blending; its inherited floating-point alpha is preserved.
This stabilizes endpoint branches such as Color Burn at exact white/black across
f64 CPU and f32 GPU math. A positive alpha smaller than half a byte still receives
the color transform. Destructive/layer comparisons allow the specified one-byte
difference from coverage interpolation and compositing quantization.

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

The implementation resamples each visible layer/mask/selection into a temporary
screen-space cache, at most 2048 pixels per side, using Rust nearest sample
centers. A step pins at most one authoritative source generation; a six MiB
scratch reservation covers mapping, pending samples and bridge uploads. Cached
generations are counted once with document/history/display buffers and may use
the existing pager. The GPU executes the full ordered layer plan over these
screen tiles and converts premultiplied float results into a bounded RGBA8 display
texture, without reading preview pixels back to JavaScript. Rotation, flip and
pan are captured in the cache key. This is an interactive display sample, not a
full-resolution export or a guarantee of identical downsampling at fitted zoom.

Initial preparation waits for the GPU queue fence as well as source-cache work.
This keeps asynchronous shader initialization inside the Preparing preview
status. Warm control updates reuse these resources, coalesce intermediate UI
settings, and acknowledge the presented viewport frame. Cancelling waits for the
active preview request to release its admission slot before a subsequent Apply.
Document/view/target changes and worker replacement invalidate the cache. Export
and autosave read only committed content. A layer-command JSON bridge is bounded
to 16 KiB before parsing. New manifest schema 3 stores adjustment settings and
sparse mask defaults inside the unchanged bounded v2 stream framing; missing
schema means legacy schema 2. Legacy single-raster v1 input is still supported.

## M3 implementation contract

M3 contains Gaussian blur, box blur, unsharp mask, sharpen and median, plus Fade
Last Filter. These destructive operations target raster layer pixels. Every Apply
builds an immutable-input candidate and commits one tile command. Pixel/all and
ancestor locks reject editing; alpha lock preserves the original alpha plane.
No smart-filter metadata or project schema change is introduced.

Gaussian has integer support `[-radius,+radius]`, sigma `radius / 3`, normalized
finite exponential weights, and separable horizontal/vertical passes. Radius zero
is identity. Box blur is a uniform square over the same support. Unsharp mask uses
the Gaussian result, adds `amount * (original - blurred)` to straight sRGB, and
changes a channel only when its absolute difference reaches the byte threshold.
Sharpen uses the same detail formula with a radius-one box. These two preserve
the original alpha. Median selects each premultiplied component's middle order
statistic in the square neighborhood. Radius limits are 64 for convolution and 5
for median; sharpen amount is 0–5 and threshold is 0–255.

Convolution/median temporaries are premultiplied floating-point sRGB. Halo samples
come from the immutable layer across tile borders, including unselected pixels;
canvas boundaries clamp. Output interpolates original and filtered premultiplied
samples by selection coverage once, then quantizes straight RGBA8. Zero coverage
returns the exact original. If output alpha rounds to zero, original hidden RGB
is retained. Sparse neighboring output tiles are included when blur can spread
alpha. Unchanged tiles retain their generations. Eight MiB covers bounded kernel
scratch. A temporary allocator reservation of at most 16 MiB, released at each
32-tile batch boundary, batches WASM heap growth within the remaining CPU room.
Unique source/candidate/history generations use shared residency and the
existing pager. Cancellation/refusal cannot partially commit or clear redo.

Fade retains immutable before, selected-result and full-filter samples and applies
opacity plus one of the existing 16 sRGB blend functions through the captured
selection. Normal fades interpolate the exact before/selected endpoints; other
blend modes use the full-filter samples and apply selection coverage once. Hidden
backdrop RGB never tints newly spread alpha. Zero opacity restores
exact pre-filter samples; Normal/100% restores the exact full result. Repeated
Fade values recompute from those endpoints. Each Fade is itself undoable. A
subsequent document mutation, history navigation, target change or replacement
ends availability. Export, project save and viewport navigation do not edit the
document. Retained Fade generations are included in residency/paging.

GPU preview reuses the visible-source compositor cache, extends it by 64 sampled
pixels on each side, and filters the active raster before its compositor step.
The target samples clamp at canvas boundaries; other layers keep transparent
outside-canvas samples. The GPU clips outside-canvas output, applies sampled
selection coverage and alpha lock, and composites the complete layer tree. The
presentation crops away the halo. GPU scratch uses one 384-square input, a float
horizontal pass, one 256-square output and selection tile. Median finds the exact
order statistic of byte premultiplied products with 16 bounded integer bisections.
Warm changes reuse sources and coalesce controls. Initial source/GPU preparation
is measured separately. Document pixels never cross JavaScript for processing.

The sampled interior is at most 1920 per side and never denser than roughly one
sample per document pixel when zoomed in. Filter radii scale to the sampled view
and round to whole sampled pixels. As with M2, fitted/rotated/resampled preview is
an interactive approximation; final Apply uses full-resolution document samples.
The two panels share preview ownership. Document/view changes, cancellation and
worker replacement release or rebuild temporary state; export/autosave only see
committed content. CPU and GPU budgets include preview resources.

While a command owns worker admission, frame ticks continue publishing progress
snapshots but do not submit ordinary viewport/selection redraws. Command GPU
composition/readback must not compete with repeated old viewport frames. A
prepared live preview's pending presentation is the exception: its frame must
present before the preview can release admission and finish its GPU fence.
Every presented frame also awaits submitted GPU work before returning its RPC.
FramePump's single in-flight transaction therefore bounds GPU completion, not
just submission, including animated selections on slow adapters. The wait uses
asynchronous queue fences without pixel readback. Commands and cancellation can
run while it waits; disposal releases pending waits, and preview cancellation
is independent of the ordinary frame fence.
Snapshots and backing dimensions are captured at submission, so a command that
arrives during the GPU wait cannot attach newer metadata to the older frame.
