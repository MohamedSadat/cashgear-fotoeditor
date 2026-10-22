# Decisions

## 2026-10-23 — P3 serialization foundation

- User selected 256 MiB history / 200 steps, configurable within the existing
  768 MiB total CPU budget. Oldest applied steps are evicted first; if at the
  baseline, farthest redo steps are dropped. An edit larger than the history cap
  can complete without a retained undo step, with visible eviction reporting.
- Autosave every 30 seconds while changed plus best-effort tab hide. Ask
  “Restore last session?” at startup. Project/recovery files store the document
  only, opening a fresh undo history. These choices were confirmed on 2026-10-23.
- Version-1 project encoding is a browser-independent Rust job in fotoeditor-io.
  It stores sparse 256² RGBA8 tiles in coordinate order and exact f64 DPI, bounded
  by the existing 128 MiB file cap. A CRC32 covers header and every tile record.
  Checksums detect accidental corruption; they are not authentication.
- The reader constructs a candidate document, rejects unsupported versions,
  malformed lengths and duplicate/out-of-range coordinates, and completes only
  after checksum verification. Integration must commit this candidate atomically.
- Project save/read jobs handle one tile per step and reserve document, file and
  bridge copies against the total CPU budget. No document pixels are processed
  in TypeScript. crc32fast is already in the approved dependency inventory.
- Save admission can evict old history to reserve serialized output and bridge
  copies. Recovery also reserves its readback buffers before allocating them;
  live worker file buffers appear in temporary residency. GPU stays separately
  capped at 128 MiB. These logical capacities exclude allocator/driver overhead.
- Consecutive resolution edits merge within one focus/pointer gesture; navigation,
  save and budget configuration end a merge run. Brush strokes never merge.
- OPFS uses unique immutable generation files. The worker flushes/closes the file,
  validates its version/coordinates/CRC in Rust, then flushes a metadata commit
  record and reads it back. Three validated generations remain. Failed writes
  remove only their own new files; startup cleans uncommitted files of inactive
  sessions. No rename or single mutable head file is the durability mechanism.
- Web Locks hold each tab's session ownership for its worker lifetime and serialize
  catalog access. Startup ignores active sessions. Restore/discard recheck ownership
  and stored metadata under those locks. Restoring retains the source until discard.
- Persistent storage is requested on the main thread and its outcome passed to
  the worker. A denial/failure is visible and does not block editing or OPFS saves.
  Quota/write failures retain completed snapshots and offer retry/project saving.
  Browser site-data clearing/eviction and OS power-loss durability are not promised.
- Autosave captures only a completed, changed document, every 30 seconds and on
  tab hide when idle. Tab-hide completion is best effort; recovery restores the
  last committed generation, with fresh undo history, after asking the user.

## 2026-10-22 — P2 command foundation

- General commands implement apply/revert and enumerate retained tile allocations.
  Tile commands store before/after Arc generations only for changed coordinates.
  Undo restores sparse absence and exact straight-alpha samples, including hidden RGB.
- History admission applies to a candidate document and checks unique retained
  document/history buffers plus temporary bytes before changing the live document
  or clearing redo. Empty and rejected commands preserve redo.
- P2 does not silently evict history. It rejects an edit that cannot fit the existing
  768 MiB CPU budget; P3 adds configurable budgets, eviction and the history panel.
  New/open document lifecycle commands start a fresh history session.
- User confirmed Photoshop B/E/I/[ / ]/X and Alt eyedropper, both pulled-string
  and weighted-average stabilizers, and image tips/textures deferred to P9.
- No pen hardware is available. Synthetic pressure/tilt tests cover the input and
  engine contracts; physical Windows Ink behavior cannot be claimed as verified.
- The user explicitly approved deferring physical pen validation and completing
  P2 with synthetic pressure/tilt coverage if every other acceptance check passes.
- Each stroke keeps a COW preview and a UNORM16 coverage mask in u32 scratch
  buffers. Flow accumulates coverage; stroke opacity is applied to the original
  samples, so overlap does not exceed the cap. Erasure changes alpha and preserves
  hidden RGB. Completion compacts exact zero tiles and admits one tile command.
- Procedural elliptical masks are prepared once per stroke, capped at 512 pixels
  of mask resolution and resampled for larger tips/dynamics. Pencil snaps to one
  document pixel. Batches visit at most 262,144 pixel positions and 512 dabs;
  the engine input queue holds at most 2,048 samples. Overflow/budget refusal
  cancels the preview and reports the error, leaving the document/history intact.
- Native raw pointer events and coalesced move fallback retain pressure, tilt and
  source timestamps, with duplicate raw/move samples removed. Mouse pressure is
  one. Low-flow brushes add a stationary dab every 33 ms. Weighted smoothing uses
  a time-dependent exponential average; pulled-string radius is in document pixels.
- Dirty rectangles update cached base-tile interiors during painting. Changed
  generations and filtered mips are invalidated; neighboring gutters refresh when
  needed for filtered views. Display scratch reserves 1 MiB within the CPU budget.
  WASM SIMD and explicit hot-loop boundaries keep the CPU kernels small enough
  for browser optimization; the same straight-alpha reference tests cover them.
- The frame pump serializes worker transactions and owns every GPU presentation.
  Input may use an unused frame slot immediately. An active preview requests the
  remaining slot after 8 ms; unchanged previews do not consume it at the start
  of the animation frame. Idle documents submit no GPU frames. Presentation
  timestamps prevent a delayed RPC reply from consuming the following frame's slot.
- The P2 hardware benchmark measures a 300 px soft round at 100% on the existing
  deterministic 6000×4000 fixture, 10% spacing, 1800 px/s and 120 Hz native input.
  Report average presentation rate plus p50/p95/p99 acknowledgement intervals and
  source-sample latency. The under-one-frame result is assessed at p95, with tails
  retained explicitly. Acknowledgement includes queueing, CPU work and GPU submission;
  compositor/scanout and physical input hardware latency are outside this measurement.

## 2026-10-22 — P1 choices and implementation

- User confirmed 6000×4000 as the measured workload and 16,384 pixels maximum per side.
- New canvas presets: 1920×1080 / 72 DPI (default), 3840×2160 / 72 DPI,
  6000×4000 / 300 DPI, A4 2480×3508 / 300 DPI, and custom. DPI range 1–2400.
- Light gray checkerboard inside a transparent canvas, dark gray workspace.
  Checkerboard is preview only; export preserves transparency and hidden RGB.
- Wheel/two-finger gestures scroll. Ctrl/Cmd+wheel and browser pinch-wheel zoom
  around the pointer. Space+drag and middle mouse pan; R+drag rotates the view.
- P1 opens PNG/JPEG/WebP in Rust, creates a single raster layer, and exports PNG.
  16-bit input is rejected explicitly until P11. Color conversion/ICC and extended
  metadata remain P8; P1 treats samples as sRGB and preserves PNG resolution.
- Document samples are straight RGBA8. Only exact all-zero tiles are sparse;
  alpha-zero pixels with hidden RGB cannot be discarded. Tile Arcs use COW and
  stable generation IDs; GPU mirrors can be evicted and rebuilt from CPU samples.
- Worker session commands atomically replace a document after construction.
  There are no pixel editing commands in P1. General command execution with
  apply/revert and undo must precede painting in P2.
- CPU admission budget: 768 MiB including current document, pending construction,
  codec/conversion reserve, history (zero in P1), export and bridge copies.
  Input/output files are capped at 128 MiB. Dense images may hit this memory limit
  before the maximum side limit. Blank maximum-size canvases remain sparse.
- GPU display atlas pages are 2048² RGBA8, with a 128 MiB residency ceiling.
  Visible tiles alone are uploaded. Invisible slots are reusable; old mip pages
  are evicted by recency within the budget. Replacing a document discards the cache.
  The initial fit overview stays resident. Navigation uses cached mips while finer
  tiles upload in approximately 3 ms CPU batches; a refining indicator reports this.
  Initial opening uploads the fit view before reporting completion. Display mips are box
  filtered in premultiplied alpha, with neighbor gutters; >=100% uses nearest.
- Logical memory diagnostics count tile buffer capacities, active job buffers,
  atlas textures and instance buffers. Hash maps/Arc headers, browser/driver
  overhead and retained WASM allocator pages are not exact process memory.
  Codec internals are conservatively reserved at admission, not directly measured.
- Import tiling and PNG export yield between chunks and support cancellation.
  The decoder call itself is synchronous; cancellation resumes after decode.
  Failed/cancelled imports leave the previous document intact.
- UI cancellation also invalidates a pending file read before worker dispatch.
  Once dispatched, worker cancellation is acknowledged before the UI reports it.
  Tests use the visible operation rather than relying on transient memory samples
  being observable on a particular software renderer frame.
- All surface presentation goes through the existing frame pump, including the
  first opened-image frame. Opening waits for that presentation acknowledgement;
  import must not render independently and acquire the same WebGPU canvas texture
  twice in one browser animation frame. Cancellation ends at atomic document commit.
- wgpu owns canvas dimension changes on both backends. Identical resizes are
  ignored; presentation resumes one animation frame after reconfiguration to let
  the transferred canvas/compositor settle. A fixed-height operation bar avoids
  resizing the viewport again when its Cancel button disappears.
- Transform round-trips use 1e-8 document-pixel tolerance as recorded in the review.
  View changes (rotation/flip/zoom/pan) never alter exported samples.

## 2026-10-22 — P0 user choices

- Product: CashGear FotoEditor; repository cashgear-fotoeditor; crate prefix fotoeditor-.
- First delivery: P0 scaffold and GPU/platform feasibility. Complete it before P1.
- Browsers: Windows Chrome and Edge, with WebGPU and WebGL2.
- Hosting: static files, served from the origin root on an HTTPS host supporting custom headers.
- Enable COOP same-origin and COEP require-corp now. Self-host runtime assets.
- Git: direct work on main, small conventional commits, phase completion tags.
- GitHub Actions CI follows the approved implementation plan.
- Desktop: the web build and WASM engine run in Tauri 2 / WebView2.
- Rust 1.99.1, Node 24, pnpm 12.11.2, React 19, Vite 8, wgpu 30.0.1,
  wasm-bindgen and its CLI 0.2.129.
- TypeScript 6.0.3: current typescript-eslint does not support TypeScript 7.

## Reviewed roadmap corrections

- Authoritative document tiles store straight-alpha samples. Premultiplied RGBA8
  cannot preserve exact low-alpha or fully transparent source RGB values.
- Design residency accounting and evictable tile handles in P1/P3. Twenty dense
  6000x4000 RGBA16 layers occupy 3.75 GiB with 256x256 tiles before history/scratch.
- Put command execution and basic undo before painting. P3 adds the history UI,
  memory-budget eviction, and robust recovery.
- Introduce versioned project serialization and basic save/reopen alongside P3 autosave.
  P8 expands formats and interoperability.
- Use recoverable snapshot generations, retain the previous valid snapshot, handle
  quota/persistence failures, and coordinate writers. A temp rename alone is not
  the full durability contract.
- Request persistent browser storage on the main thread and pass its result to
  the worker; StorageManager.persist() is unavailable in workers.
- Chunk long CPU jobs, support cancellation, and commit completed jobs atomically
  before filters/large codecs can starve the rendering worker.
- Verify PSD masks and 16-bit samples with a Photoshop fixture corpus. The proposed
  psd reader skips mask metadata and exposes RGBA8; extensions/replacement are unselected.
- Use numerical tolerances for geometric round-trips. Specify hardware, browser,
  dataset, percentiles, and cold/warm state for later performance criteria.
- P7's one-resample criterion applies within a single uncommitted transform session;
  two committed destructive transforms each resample.
- Split P6 and P9 into independently accepted milestones. A phase need not fit one session.
