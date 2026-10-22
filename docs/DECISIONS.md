# Decisions

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
