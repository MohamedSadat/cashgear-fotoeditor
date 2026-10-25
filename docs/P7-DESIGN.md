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
