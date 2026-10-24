# P5 selections, fill and clipboard

Source: original roadmap printed page 14, with the four 2026-10-24 product choices
in DECISIONS.md. Work directly on main; finish this phase before P6.

## State and commands

An active selection is an optional shared Gray8 tile surface. None means tools
are unrestricted; an active zero mask means no pixel can be edited. A default
255 surface represents Select All sparsely. Coverage is always zero beyond the
document. Partial coverage weights brush/eraser/fill once at final application.
Zero coverage preserves all four original channels, including invisible RGB.

Selection changes use exact COW commands in the same history as pixel edits.
The document enumerates selection generations for unique residency accounting
and paging. The active selection is session editing state: v1/v2 project and
recovery files keep their existing layer-only representation and reopen with no
active selection. Named selection channels are the roadmap's optional follow-up.
The clipboard belongs to the worker session, not the project or undo history;
cut changes pixels through a command only after copying succeeds. Clipboard and
selection previews contribute to temporary/document/history budgets as applicable.

## Independent Rust processing

fotoeditor-select depends on core only (and already approved serde for typed
options). Shape rasterization, coverage algebra, selection morphology/feather,
scanline flood selection, fill and clipboard pixel extraction stay in Rust.
The browser bridge supplies tile hydration and runs bounded steps, yielding and
checking cancellation. Cold handles are never sampled before hydration. Jobs
reserve working/output buffers before allocation; completed candidates become
commands atomically. A failed or cancelled job retains the document and redo.

Selection algebra uses coverage max for add, saturating subtraction, min for
intersection and 255-minus for inversion. Shapes use deterministic area sampling
for anti-aliasing. Wand/bucket use seed-relative maximum-channel sRGB distance
including alpha, with four-connected contiguous traversal or global matching.
Transparent samples compare visible premultiplied color and alpha so hidden RGB
does not alter visual matching; stored samples remain straight and exact.

Mask feathering uses three separable box passes approximating a Gaussian with
sigma equal to the requested radius. Grow/shrink use separable square windows;
smooth removes small protrusions/holes with opening and closing. Replicated
canvas boundaries preserve a selection touching the canvas edge, consistent
with Adobe's default `applyEffectAtCanvasBounds=false`. This does not establish
radius equivalence with Photoshop: that visual comparison remains pending.
See [Adobe's Selection API](https://developer.adobe.com/photoshop/uxp/ps_reference/classes/selection/).

Quick Mask brushes edit a grayscale selection candidate through the existing
bounded stroke machinery. Quick Mask itself is a UI/view mode; its red overlay
and marching ants are GPU presentation only. The ants follow the mask's 50%
contour, include document boundaries and animate through the normal frame pump.
No selection overlay appears in image export or thumbnails.

## Integration and clipboard

React/zustand owns active tools, settings and pending geometric gestures. The
worker/WASM owns committed selection, Quick Mask view mode, pixel candidates and
clipboard samples. Pointer coordinates use the existing document transform,
including the worker's logical viewport dimensions at fractional DPR.
M/L/W/G/Q and standard select/edit shortcuts coexist with painting/navigation.

System image transfer uses Clipboard API PNG bytes. Encoding/decoding, selection
extraction and positioning happen in Rust; TypeScript handles bytes and API
permission/errors only. The internal clipboard retains exact straight RGBA and
source coordinates even when the system clipboard is unavailable. Browser
clipboard conversion may normalize fully transparent RGB; report this boundary
instead of claiming arbitrary external applications preserve hidden samples.
Paste always adds a new layer; regular paste centers it in the document and
Paste in place retains its copied origin. Clipboard images keep the 128 MiB image
limit and admission checks; no document-sized JavaScript pixel arrays.
See [Clipboard API requirements](https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API)
for secure-context, activation and permission behavior; rejection is a visible
system-clipboard failure while the internal clipboard remains usable.

## Verification and evidence

Test selection algebra, edge coverage, tile seams, sparse all/empty, morphology,
wand connectivity/tolerance/all-layer sampling, fill/locks, cut/paste and exact
undo/redo/branch rejection, including partial alpha and hidden RGB. Exercise
cold paged inputs, cancellation and budget refusal. Browser/desktop tests must
inspect rendered overlays and exported samples on both GPU APIs, including
selection clipping under rotated/flipped views and clipboard failure fallback.

Use the credited NASA 6000×4000 photo fixture for measured wand trials, recording source,
tolerance, selected pixels, warm/cold state, end-to-end and kernel time, logical
residency, browser/adapter and p50/p95/p99. The under-200 ms criterion cannot be
replaced by a software result or a tiny connected region without disclosure.
Run existing brush/navigation regressions and the full required verification
matrix. Photoshop feather reference remains unavailable; prepare a reproducible
edge fixture and record the comparison as pending until a reference is obtained.

Only publish p5-select after all acceptance checks, remote checks and deployment
pass. Report shipped/deferred work and measurements; propose P6 and stop.

## Bounded integration details

The wand matches into packed bitplanes, floods four-connected scanline spans,
then converts coverage one tile at a time. Opaque WASM membership uses SIMD128;
translucent samples use the same scalar visible-color distance. A packed 3×3
Gaussian edge pass is checked independently against its scalar formula at word,
tile and canvas seams. Tolerance 255 with no domain has an exact sparse all-canvas
shortcut; report that case separately from matching/traversal. Bucket traversal
treats zero selection coverage as a connectivity boundary, then applies partial
selection coverage once when filling.

Each worker slice runs resident bounded steps for approximately four milliseconds,
then yields to cancellation and presentation. Cold source handles are hydrated
before sampling. Before a wand job, the WASM bridge admits and releases up to
32 MiB of temporary allocator capacity to avoid repeated small memory-growth
calls while retaining selection history. It counts against CPU admission during
that call, but sampled frame residency may miss it. Retained linear-memory pages
and browser/driver allocations are outside the logical residency contract.

GPU ants/Quick Mask use a screen mirror limited to 2048 pixels per side, RG8
coverage plus inside-canvas data (at most 8 MiB each on CPU/GPU). The mirror is
resampled in Rust only after view, selection or Quick Mask stroke changes;
the GPU animates the contour. Large/high-DPR display mirrors can approximate a
thin contour while authoritative selection/edit samples remain exact Gray8.
Preparation admits overlapping old/new buffers before allocation.

The worker distinguishes session revision from persisted content revision.
Selection-only history navigation preserves the composite cache, thumbnails
and recovery timestamp. Projects/recovery omit active selections; selection
history remains usable during the current session. Copy/cut targets layer pixels;
the UI asks the user to return from a layer mask or Quick Mask first. Fills and
brushes can edit those mask targets. Own-origin system clipboard PNGs carry a
custom token when supported, allowing exact internal samples to be reused;
external PNGs go through the bounded Rust decoder and always paste a new layer.

Geometric outlines are capped at 4096 points. Reaching the cap cancels visibly;
Escape, blur, capture loss, tool changes or document revision changes discard
unfinished gestures. Enter or double-click completes a polygon.

## Feather comparison preparation

Run `cargo run --release -p fotoeditor-io --example feather_fixture` to produce
513×321 Gray8 PNGs in `bench/results/feather/`. The input combines a rectangle
[0,80,260,240] and an anti-aliased ellipse [300,40,490,280], covering canvas-edge
and 256-pixel tile-seam cases. Input SHA-256:
`88b41ebca2ccbef2a1042efe77c9c53dfbaf6b5d24823e6494bf0d31df24eb8e`.
Independent outputs use radii 1, 3, 8 and 24. Load the input as a Photoshop
selection channel and export the feathered mask at each named radius with the
Photoshop version and canvas-boundary option recorded. Compare aligned raw
coverage, edge profiles and visual overlays. No Photoshop output is currently
available; the prepared FotoEditor images do not satisfy that comparison.
