# P8 image export

Export image provides PNG, JPEG quality, WebP quality/lossless, a selectable
JPEG matte and optional sRGB ICC embedding (enabled by default). Quality defaults
to 85. Preview encoded size runs the actual encoder and reports the resulting
file length; Download image reuses the matching file. Option, document revision
or active-layer changes invalidate that preview. Export PNG remains a shortcut.

All sources use the visible canvas window, excluding retained outside-crop
samples. Full document exports the ordinary composite even in mask view. Active
raster layer exports that layer in isolation with its enabled mask, opacity and
fill; visibility, clipping and blend/backdrop settings do not alter this isolated
image. Groups/adjustments are refused as isolated sources. Selection on canvas
is canvas-sized, applying selection coverage to alpha; absent/empty selections
are refused. Straight hidden RGB remains intact for lossless formats. JPEG mattes
RGB in the document's chosen sRGB blend space and always has opaque output.

## Jobs and memory

Rust reads at most 32 scanlines per capture/PNG step and enumerates source/mask
tiles for the existing pager. PNG retains its bounded streaming encoder, so a
sparse maximum-size canvas does not need a full RGBA buffer. JPEG/WebP capture
immutable visible RGBA, admitting both the capture and bridge copy before work.

The engine persists/evicts eligible sample generations and admits the transferred
input, bounded bridge/file scratch and a complete codec heap allowance against
the remaining shared 768 MiB CPU budget. A fresh CPU-only WASM instance encodes
each JPEG/WebP job. Its allocator tracks live allocation sizes and conservatively
charges both blocks during realloc; exceeding the ceiling fails the isolated
job. Logical accounting excludes allocator arenas and browser/driver overhead,
as in the existing engine contract. PNG native output capacity remains included
in engine residency until released.

The UI creates the sibling codec worker and transfers a MessagePort. Immutable
samples and the output handle then travel directly from engine to codec. The UI
does not process samples or own codec state. Nested worker loading failed in the
packaged WebView2 custom protocol; the sibling route resolves the observed
failure without changing CSP, matching the upstream
[Tauri/WebView2 issue](https://github.com/tauri-apps/tauri/issues/15755).

Codec output remains in WASM and is copied in chunks of at most 1 MiB to an OPFS
sync handle, flushed/closed and returned as a file-backed File. The main UI does
not materialize a full encoded ArrayBuffer. Cancellation terminates the codec;
file-operation cancellation waits for pending output abort/cleanup before
releasing reservations and accepting another operation. No export mutates the
document or its undo/redo. Output remains capped at 128 MiB. Lossy WebP's VP8
dimensions are limited to 16,383 per side; PNG/lossless WebP retain 16,384.

webpkit supplies bare VP8. The wrapper writes the standard WebP container and an
exact uncompressed alpha plane, avoiding an additional alpha-compression working
set. image-webp supplies exact lossless RGBA. JPEG and PNG preserve supported DPI;
WebP exports currently omit resolution metadata. Temporary output files live in
the session directory until abandoned-session cleanup, like project downloads.

## Reference checks

Native tests cover odd crop/tile edges, selected coverage, hidden RGB, mask and
opacity/fill, empty/group refusal, admission refusal, exact lossless WebP,
quality-dependent sizes, JPEG mattes, ICC opt-out and the 16,383/16,384 boundary.
The reference example and scripts/export-reference.py independently decode six
tagged/untagged outputs through Pillow 12.3.0, libjpeg 8.0 and libwebp 1.6.0:
lossless RGBA and both WebP alpha planes are exact, ICC presence/absence agrees,
and JPEG is opaque with 300 DPI. These are independent codec checks, not claims
that lossy RGB must equal its source.

The focused browser checks compare preview lengths with actual downloads and
cover source/crop semantics, exact document preservation, real codec heap refusal
and cancellation with preserved redo. Expanded WebView2 checks exercise all four
export modes and ICC markers on both APIs. See PROGRESS.md for the current
checkpoint and remaining complete-phase/performance/remote gates.
