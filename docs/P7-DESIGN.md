# P7 transforms, crop, resampling and guides

Started 2026-10-25 on clean synchronized main at `5a22595`, after reading
AGENTS.md, PLAN.md, PROGRESS.md, DECISIONS.md, P6-M4-REPORT.md and the complete
original roadmap P7 section on printed pages 15-16. The user authorized P7.
No worktree or agents. All four product questions are answered: Lanczos3 default,
non-destructive crop, warp/puppet warp after v1, content-aware scale outside v1.

## Canvas retention

The retained tile extent stays in `Document.width/height`; an optional integer
`CanvasRect` identifies the visible window. Ordinary crop changes metadata only,
preserving every raster/mask generation and hidden RGB. Reveal All resets the
window. Image export visits only visible rows/columns. Streamed schema 4 stores
the window and document-coordinate guides, while reading schema 2/3 unchanged.
Brush dabs clip to the visible window. Layer processing retains its full extent;
resizing/rotation/movement use candidate jobs and expand/rebase retained extents
as needed, within the existing side and residency limits. Pixel positions inside
processing remain retained-document coordinates; controls and rulers label them
relative to the visible canvas origin. No nested crop snapshots or unaccounted
hidden-pixel archive is introduced.

## First independent increment

`fotoeditor-core::TransformMatrix` establishes row-major document-edge affine and
projective geometry, composition, inversion, bounds and convex-quad handles.
It rejects nonfinite/singular matrices, collapsed/crossed handles, projective
horizons and coordinates outside the supported range before any pixel allocation.
Right-angle rotations use exact coefficients. The four interpolation names are
defined without choosing a default. No new dependency is introduced.

Four Rust tests cover independent corner targets, reflected perspective quads,
interior inverse round-trips, homogeneous scale invariance, accumulated +10/-10
rotation, exact right-angle composition, invalid inputs and 500 randomized affine
round-trips within 1e-8 document pixels. This is geometry coverage, not the later
pixel-resampling or visual acceptance. The new APIs are not yet wired into engine
commands, samplers or controls. No P7 Apply/preview performance is claimed.

## Resampling and whole-document geometry increment

`SurfaceResampleJob` uses fast_image_resize 6.1.0's typed F32x4 API for
axis-aligned image resizing, with globally phased padded source blocks. The
dependency enables only `std`, under MIT OR Apache-2.0; it adds no browser/GPU
types to the processing crates. Temporary buffers contain premultiplied floats,
and quantization/unpremultiplication happen once at final RGBA8 reconstruction.
An early U16 implementation failed the independent reference because integer
intermediate clipping changed ringing edges; the float implementation passes.

Arbitrary matrices use the four corresponding Rust kernels, with Jacobian-based
support expansion for reductions. Integer permutations copy complete straight
samples, including hidden RGB, without interpolation. Identity shares original
generations. Other zero-alpha outputs keep the nearest source's hidden RGB;
outside the retained source they use the explicitly selected field default.
Selection growth uses zero outside even when its sparse source default is 255.

FIR input and intermediate blocks are bounded to 2 MiB apiece within a conservative
16 MiB scratch reservation. Generic steps yield after 256 output pixels or a
source-tap budget, visiting one tile dependency at a time for extreme reductions.
Candidates compact all-default tiles and expose every retained generation for
shared residency and durable paging. The worker checks CPU admission, hydrates
dependencies, yields, persists growing candidates and publishes one final command.

`GeometryJob` handles image size/DPI, anchored canvas size, canvas rotation and
flipping throughout nested raster layers, masks, selection and guides. Anchored
canvas growth rebases retained samples with an exact integer translation; shrinking
within the retained extent changes only the visible window. Half-pixel anchor ties
round away from zero. Image size scales retained content as well as the visible
window, aligning its new origin to an integer and retaining the requested visible
dimensions exactly. All jobs use COW document commands and preserve redo on no-op,
refusal and cancellation.

Right-angle rotations exchange guide axes and map positions exactly. Arbitrary
rotation keeps axis-aligned guides at their canvas-relative offsets, clamped to
the new retained extent; the format does not introduce angled guides.

The independent image 0.25.10 float Lanczos3 implementation verifies a 1030x518
opaque RGBA fixture at 50%. Maximum RGBA error is `[1, 0, 1, 0]`, within two.
The fixture formula/version and full settings are in `resample_reference.rs`;
raw output is `bench/results/p7-lanczos-reference.log`. Separate tests cover
fractional alpha/hidden color, all four kernels, odd tile/block boundaries,
perspective nearest sampling, fractional bilinear reference, cold extreme
reduction, exact permutations, nested masks/selections and command restoration.

## Immutable transform sessions and Move

`TransformSession` retains one immutable document, a raster target, interpolation
and a final absolute homography. Handle changes compose geometry; they do not
resample document tiles. A pageable bounds scan includes meaningful straight RGBA
samples (including hidden RGB), linked-mask differences or selected coverage
inside the visible canvas. This avoids accumulating empty tile padding during
movement and validates perspective horizons over the occupied source domain.

Whole-layer transforms carry the linked mask. Selected-content transforms lift
and clear byte-quantized coverage once, transform that lift once and source-over
it once onto the cleared base; their existing layer mask stays fixed. Identity
preserves generations and redo. Full-tree rebasing retains outside content and
preserves the screen anchor when visible dimensions do not change. All sources,
lift/clear buffers, candidates and scratch participate in shared paging/accounting.

The live GPU preview stores immutable raster/selection/mask textures at up to
1536 pixels on the longest retained side. It renders through the existing layer
compositor, preserving groups, clipping, blend and opacity. Four sampling kernels
operate in premultiplied temporary floats; preview support is capped at four
cache pixels per axis for interactive cost. This is an approximate preview on
large documents and strong reductions. Apply uses the full-resolution Rust job,
its uncapped reduction support and one final resample. No full-resolution preview
or fixed P7 latency promise is implied.

At actual pixels, the masked 10-degree rotation preview is checked against the
committed canvas within one channel value, after rebuilding through a rotated and
flipped camera. A rotated normal viewport uses interpolated atlas coordinates;
the preview samples document pixel centers. Near integer coordinate boundaries
their raster precision can choose neighboring pixels (three pixels exceeded one
channel value, with maximum error three, in the recorded small fixture after
fixing the checkerboard origin).
The strict sampling comparison therefore uses a pixel-aligned camera. The
transparency checkerboard anchors to the visible origin, so retained-extent
rebasing cannot shift its phase across Apply.

Source uploads and background preparation hydrate/yield in bounded steps. View
changes rebuild the viewport cache against the same source/matrix. GPU completion
fences, captured frame snapshots and preview acknowledgements remain in use.
Cancellation drops prepared caches; exports can read the unchanged document
without destroying a transform session. Pixel mutation invalidates stale sources.

Move provides optional alpha/mask/group/clipping-aware layer hit testing, one
command per drag, and document-axis arrow nudges (Shift: ten pixels). Eight
handles plus a rotation handle support scale, skew, distort and perspective;
Shift preserves scale proportions and snaps rotation to 15 degrees. Enter applies,
Escape cancels. Screen-distance snapping uses document guides and canvas edges,
through the rotated/flipped/zoomed view. Controls expose the four interpolation
modes, translation, scale, rotation and skew. New sessions default to Lanczos3.

## Crop authoring and straighten

The Crop tool (`C`) keeps an editable draft, with eight resize handles, Alt-drag
movement, a rule-of-thirds overlay and Free/Original/1:1/4:3/3:2/16:9/9:16 ratios.
Shift constrains a free rectangle to a square. Guide/canvas snapping uses the same
six CSS-pixel tolerance through document coordinates, including rotated/flipped
views. Enter applies, Escape cancels, and content revision changes discard stale
drafts. Controls label positions relative to the visible canvas origin.

The straighten-line gesture records its angle relative to the closest document
axis; the numeric correction is limited to ±45 degrees. The draft shows the line
and angle; image rotation happens on Apply. `CropStraighten` rotates the entire
retained layer tree once about the draft rectangle's center, rebases its enclosing
extent, then uses that axis-aligned destination rectangle as the visible window.
It includes masks, selection and guides in one candidate and one command. Reveal
All recovers outside content in its transformed coordinates; undo restores the
exact original samples and metadata. A zero angle shares original tile generations
and performs the same non-destructive crop as the metadata-only command.

Display mip interiors and gutters clamp to the visible crop window before
filtering. Retained outside pixels therefore cannot bleed into minified edges.
The ordinary full-window fast path stays unchanged. Existing geometry/view sync
invalidates cached display mips when crop metadata changes. Bounded neighbor
hydration remains sufficient, and authoritative document samples are untouched.

Worker replacement explicitly resets geometry control ownership and pending
state. Late results from an abandoned worker cannot publish crop state into its
replacement. The diagnostic after-step budget fault refuses an allocation after
a real processing step, even if a tiny job releases its scratch before a next
allocation; this makes the atomic candidate-failure check deterministic.

## Processing and integration contracts

Geometry and pixel processing remain browser/UI/GPU-independent Rust. TypeScript
may manipulate handles, matrices and guides, never document pixels. The engine
worker owns immutable session sources, candidate jobs and GPU resources; React
and zustand own controls and interaction state. Existing completed-GPU fences,
captured frame snapshots/preview acknowledgements, busy redraw scheduling and
submission timestamps remain intact.

Use document-edge coordinates: pixel centers are `(x + 0.5, y + 0.5)`. Matrices map
source edges to destination edges. Validate finiteness, invertibility and absence
of projective horizon crossings over the source bounds before allocation. Accumulate
all handle/control updates against an immutable source; preview changes never
commit resampled tiles. Enter applies one complete candidate command, Escape
discards the session and preserves redo. Exact identity, integer translations,
flips and right-angle rotations bypass interpolating arithmetic.

Provide nearest, bilinear, bicubic and Lanczos3. Axis-aligned resize uses
fast_image_resize with bounded source regions, filter padding and global phase;
arbitrary matrices use the corresponding Rust sampling kernels. Filter temporary
buffers use premultiplied alpha; authoritative tiles stay straight RGBA8. Exact
sample permutations preserve hidden RGB. Zero-alpha output uses a documented
hidden-RGB policy and tests, rather than silently discarding all hidden samples.
Downscaling expands filter support to avoid aliasing and tile seams.

Jobs expose next-step dependencies for OPFS hydration, visit every retained source
and candidate generation for shared residency, yield between bounded chunks and
publish only after full validation. Cancellation/refusal/errors never publish a
partial document or clear redo. Whole-document commands include dimensions,
layers/masks, selection and guide/crop metadata. Existing limits and file caps
remain unchanged. New format fields require versioned backward-compatible reading
and bounded validation; reopening starts fresh history as previously decided.

Move supports auto-selection through rendered alpha, document-axis nudging and
position/ancestor locks. Layer transforms include the linked mask; selection
transforms lift selected coverage, clear the source once and composite the
transformed samples once, including fractional coverage. Canvas operations affect
the complete layer tree and masks. Guides snap handles in screen-pixel tolerance
after conversion through the existing view transform.

## Verification contract

PLAN.md contains the complete phase acceptance list. Independently verify matrix
round-trips, singular/horizon rejection, exact permutations, sampling kernels,
alpha edges/hidden RGB, selection coverage, masks/groups/locks, crop retention,
guide persistence, paging, refusal/cancellation and exact undo/redo. A session
rotation of +10 then -10 degrees must match a single final resample; two separately
committed destructive commands are explicitly outside that one-resample promise.

Compare 50% Lanczos3 output with an independently implemented reference, recording
fixture/settings/version and maximum channel error (at most two). Retain visual
inspection at tile/canvas boundaries. Hardware preview and Apply measurements use
the resident 6000x4000 photo, separate first-use from warm samples, report p50/p95/
p99, logical residency and actual adapter identity. Software proves function only.

Finish only after all repository, hardware browser, WebView2, size/license and
exact-final-head remote gates pass; inspect artifacts for failed attempts, skips
and retries. No completion tag before those gates. Carry forward the open P5
Photoshop comparison, pen deferral and the measured legacy WebGL2 brush/completion
latency limits honestly. Do not terminate unrelated processes for benchmarks.
