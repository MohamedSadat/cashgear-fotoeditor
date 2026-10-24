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

React/zustand owns active tools, settings, pending geometric gestures and Quick
Mask display mode. Worker/WASM owns committed selection, pixel candidates and
clipboard samples. Pointer coordinates use the existing document transform.
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

Use the existing 6000×4000 fixture for measured wand trials, recording source,
tolerance, selected pixels, warm/cold state, end-to-end and kernel time, logical
residency, browser/adapter and p50/p95/p99. The under-200 ms criterion cannot be
replaced by a software result or a tiny connected region without disclosure.
Run existing brush/navigation regressions and the full required verification
matrix. Photoshop feather reference remains unavailable; prepare a reproducible
edge fixture and record the comparison as pending until a reference is obtained.

Only publish p5-select after all acceptance checks, remote checks and deployment
pass. Report shipped/deferred work and measurements; propose P6 and stop.
