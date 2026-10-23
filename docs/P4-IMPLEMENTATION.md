# P4 implementation notes

The engine worker owns the layer tree, commands, paged samples, composition and
GPU objects. React owns the panel selection and thumbnail URLs. TypeScript moves
file chunks and metadata; all document and thumbnail pixel work remains Rust.

Layer commands retain immutable RGBA8/Gray8 generation handles. Duplicate layers
share generations until editing. History can restore cold generations. A paged
generation can be evicted only after its exact payload has been flushed and
verified against readback bytes. Editing forks it without changing its backing.

Working packs have random immutable names in an owned OPFS session directory.
Web Locks protect live sessions; another worker collects abandoned working
directories only after obtaining their ownership lock. Within a session, files
remain while referenced by document, undo or pending jobs. Recovery generations
are self-contained streams in the existing coordinated recovery catalog and do
not depend on working-pack lifetimes. Three validated generations are retained.

The worker caches immutable File snapshots and hydrates bounded chunks. Residency
admission is cached between allocations and invalidated by document/job changes
or eviction. Hydration increments that admission estimate. Public memory samples
recount unique live buffers. These are logical capacities, excluding allocator
retention, map/Arc headers, browser and driver overhead. Ordinary paging reserves
2 MiB for transport; streamed codec jobs reserve 16 MiB. GPU readback reserves
4 MiB. CPU/GPU ceilings remain 768/128 MiB.

Private browser profiles may lack enough storage for a dense project. Tests found
Chromium returning an oversized unsigned write count when storage was exhausted.
Every write now checks both progress and the upper bound. Readback length and CRC
checks remain mandatory. Storage failures retain resident generations and prior
completed recovery files. The hardware dense-workload harness uses fresh
persistent profiles and reports their storage estimates.

V2 projects preserve names, properties, groups, masks, shared tile references and
exact straight-alpha samples. Rust validates the complete candidate before
replacement. Manual opening copies bounded slices into an immutable OPFS backing
and validates the flushed readback. Large manual saves use a writable file handle;
they never assemble a multi-gigabyte Blob or bridge array. Small legacy projects
retain v1 compatibility and their 128 MiB buffered limit. The v2 cap is 4 GiB,
including framing. History and view state remain excluded.

The CPU reference and WGSL compositor implement sixteen sRGB blend modes,
pass-through and isolated groups, masks and clipping. Portable fragment passes
use float32 premultiplied intermediates and one-tile readback. Up to 80 MiB is
reserved for the compositing pool, leaving a bounded viewport atlas allowance.
The displayed composite is a separate CPU cache. Stroke updates preserve
unaffected generations and invalidate changed regions and neighboring gutters.
Panning reads cached composite tiles and mips rather than recompositing every
source layer. Cold source reads and composition are measured separately.

Masks use white-default Gray8 samples. Painting uses the regular bounded brush
kernel through a Rust grayscale adapter. Alt-click displays a mask; image export
still uses document composition. Raster mask application preserves hidden RGB.
Applying an isolated group's mask rasterizes that group. A pass-through group
must first be changed to Normal because its appearance depends on its backdrop.
Merge down renders the adjacent pair; merge visible retains hidden nodes; flatten
produces one raster. These mutations retain exact command snapshots for undo.

Thumbnails evaluate a maximum 64-pixel long edge in Rust and return PNG bytes.
They refresh for new layers, the active layer and its ancestors after a completed
stroke. Thumbnail work yields and defers to editing. It does not refresh during
an active stroke.

Reproduction commands:

- `pnpm test:e2e` includes GPU readbacks, layer controls, masks, streamed roundtrip
  and interrupted/quota recovery alongside the previous phase regressions.
- `cargo run --release -p fotoeditor-io --example layers -- bench/results/fixtures/layers-50.cgfoto 4000 3000 50`
  generates 9,600 distinct dense tiles using seed `0x9e3779b9`.
- `cargo run --release -p fotoeditor-io --example validate_layers -- bench/results/fixtures/layers-50.cgfoto`
  validates the real file while retaining zero resident sample buffers.
- `node scripts/layers-bench.mjs` measures both installed browsers and backends.
  It requires the normal document renderer, hardware adapters and all 50 layer
  thumbnails before collecting warm panning trials. OS file caches are not flushed.
- After building the desktop shell, PowerShell
  `$env:P4_DESKTOP_DENSE='1'; pnpm desktop:smoke` additionally saves/reopens the
  real dense fixture through a WebView2 writable OPFS file handle on WebGPU and
  compares exported PNG hashes. Ordinary desktop smoke includes both APIs,
  blend readbacks and a byte-identical small layered streamed roundtrip.

Acceptance evidence and the phase completion status belong in PROGRESS.md.
