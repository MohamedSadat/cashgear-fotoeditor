# Decisions

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
- Chunk long CPU jobs, support cancellation, and commit completed jobs atomically
  before filters/large codecs can starve the rendering worker.
- Verify PSD masks and 16-bit samples with a Photoshop fixture corpus. The proposed
  psd reader skips mask metadata and exposes RGBA8; extensions/replacement are unselected.
- Use numerical tolerances for geometric round-trips. Specify hardware, browser,
  dataset, percentiles, and cold/warm state for later performance criteria.
- P7's one-resample criterion applies within a single uncommitted transform session;
  two committed destructive transforms each resample.
- Split P6 and P9 into independently accepted milestones. A phase need not fit one session.
