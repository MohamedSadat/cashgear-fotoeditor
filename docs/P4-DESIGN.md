# P4 layers and bounded residency

## Approved workload

Fifty distinct, fully populated 4000×3000 RGBA8 layers use 2,400 MiB after
256² padding (16×12×50 tiles). Raw in-bounds samples alone use 2,400,000,000 bytes.
Neither fits the 768 MiB logical CPU budget. Fifty dense Gray8 masks add 600 MiB.
The 128 MiB GPU budget cannot retain every layer either. The user approved paging
and a 4 GiB streamed project cap; RAM/VRAM and image-file caps remain unchanged.

P3 has stable COW generation IDs and evictable GPU mirrors, but its authoritative
`Arc<Tile>` buffers are all resident. Its project reader/writer retain whole files.
Those are useful foundations, not an existing CPU pager or streaming codec.

## Implementation order

1. Browser-independent immutable tile-generation/pager contracts and accounting;
   strict streamed record framing and checksums. Keep P3 behavior working.
2. Layer/group/mask tree, validated command operations and exact shared-generation
   undo; Rust reference composition and a matching portable fragment WGSL path.
3. Worker OPFS tile backing, bounded hydration/eviction, incremental composite
   tiles/mips, and streamed project/recovery integration.
4. React layer controls and active paint/mask target, locks, thumbnails and merge.
5. Golden, command, storage-failure/crash and hardware workload acceptance; then
   the full browser/desktop/CI/deployment matrix and completion tag.

No subagents or worktrees. Commit only passing increments directly on main.

## Storage and memory contract

- Logical generation identity is independent of its current resident buffer.
  A generation is immutable once referenced by a document/command. COW writes
  create a new generation. Eviction changes residency, never document meaning.
- Cold authoritative samples use worker-owned OPFS records with exact bytes,
  length and checksum. Only successfully written and validated immutable records
  permit CPU eviction. Failed writes retain resident samples and report the error.
- Pin buffers needed by an active operation. The pager must reject an allocation
  before exceeding the shared document/history/temporary budget; dirty, unbacked
  or pinned generations cannot silently disappear. Count shared generations once,
  including RGBA layers, Gray8 masks, undo and pending candidates.
- Hydrate/composite in tile batches. Reserve conversion/bridge/I/O buffers before
  allocation, yield between batches and honor cancellation before atomic commit.
  Storage quota, lack of OPFS, corrupt records and I/O errors remain visible.
- Panning uses cached composite tiles/mips, within the existing GPU ceiling.
  Layer edits invalidate affected composite regions. A cold composite must actually
  read every contributing visible layer; report that cost separately. Never claim
  that blending 2.4 GB of source data occurs every 16.67 ms.
- Disk backing and recovery generations share immutable tile records where safe.
  Retain records referenced by live document, undo, pending candidates or completed
  snapshots. Garbage collection cannot remove a committed generation's samples.
  Web Locks and session ownership remain part of the recovery contract.

## Project and recovery format

Use bounded record streaming and u64 total lengths (4 GiB includes all framing).
Metadata describes a validated layer tree, stable IDs, names, properties, masks,
and ordered tile references. Payload records contain exact straight RGBA8 or Gray8
samples, with strict type/coordinate/length/duplicate checks and checksums. A final
integrity record covers the stream; parsing builds a separate candidate in OPFS.
Commit only after full validation. Read existing v1 single-raster files unchanged.
History and view state remain excluded, as previously approved.

Large manual projects require a writable streamed file destination in Chrome,
Edge and WebView2; bounded slices feed project opening. Small-file fallback must
have an explicit bounded admission check. Never build a multi-gigabyte `Blob`,
`Vec<u8>` or bridge array. Recovery writes/validates a stream, then publishes the
separately flushed/readback commit record, retaining three validated generations.
An interrupted candidate must leave the prior live document and completed saves
usable. Storage capacity is separate from resident memory and must be reported.

## Composition contract

Preserve authoritative straight-alpha samples including hidden RGB. Temporary
composites are premultiplied. Use the 16 modes from the W3C compositing reference:
Normal, Multiply, Screen, Overlay, Darken, Lighten, Color Dodge, Color Burn, Hard
Light, Soft Light, Difference, Exclusion, Hue, Saturation, Color and Luminosity.
Reference: <https://www.w3.org/TR/compositing-1/#blending>.

Pass-through groups preserve the parent backdrop; isolated groups start clear.
Masks, clipping and opacity must have explicit CPU/GPU matching tests, including
partially transparent backdrops and group boundaries. Source records must not be
overwritten by display quantization. Tests must cover CPU/GPU agreement and
independent expected outputs, rather than only comparing duplicate formulas.

## Acceptance evidence

Use actual distinct dense layers; record deterministic generator/seed and hashes.
Measure cold construction/open, first composite, warm pan and invalidation/refine
separately, with storage bytes and peak logical document/history/temporary/GPU
residency. Use installed Chrome/Edge on both real hardware backends; retain
p50/p95/p99 timings and machine/browser/adapter data. Software results prove
function only. Keep ports 4187/4188 isolated and freeze dist while tests serve it.

All acceptance items remain open until implementation and measured verification.
