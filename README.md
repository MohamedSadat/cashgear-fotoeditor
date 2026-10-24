# CashGear FotoEditor

A focused photo editing and painting workspace. Open PNG, JPEG and WebP images,
create transparent canvases, paint on layers and masks, and export lossless
RGBA8 PNG. Save editable `.cgfoto` projects and recover completed browser autosaves.
The worker-owned Rust engine processes paged tiles and displays them through
WebGPU or WebGL2.

P5 adds selections, Quick Mask, fill/bucket and image clipboard editing on top
of P4's layers, groups, masks and streamed projects. P5 acceptance is still in
progress; the Photoshop feather comparison is pending. See
[progress](docs/PROGRESS.md) and [selection contracts](docs/P5-DESIGN.md).

P6 adds brightness/contrast, levels, master RGB curves, exposure and hue/saturation,
an active-raster histogram, and editable adjustment layers. Use Adjust in the
inspector, change controls, then Apply adjustment, or choose Create adjustment
layer above selection. Select an adjustment layer to edit and save its settings.
A selection becomes its editable mask. Each confirmation creates one undo step.
Authoritative alpha and fully transparent hidden RGB remain exact; the histogram
uses stored alpha weights before layer masks/effects.

Enable Live viewport preview to see changes without editing the document. Its
initial preparation includes paging and GPU initialization; later control changes
reuse the visible source cache. Preview samples the visible view at a maximum
2048 pixels per side. Final Apply processes the full-resolution selection.
Uncheck preview to cancel; saving pixels/projects always uses committed content.
Adjustment layers work with groups, masks, clipping, opacity and blend modes and
remain editable in project/recovery files.

Use Filters in the inspector for Gaussian blur, box blur, unsharp mask, sharpen
and median. Live filter preview covers the visible area; Apply filter processes
the full selected area as one undoable edit. Fade last filter changes opacity and
blend from the original filter endpoints, until another document edit or history
navigation. Destructive filter results persist in projects and recovery. See the
[M3 report](docs/P6-M3-REPORT.md). The remaining adjustments/filters and phase-wide
acceptance belong to M4. See [milestone contracts](docs/P6-DESIGN.md).

Use New canvas or Open image, or drop an image into the viewport. Wheel/two-finger
gestures scroll; Ctrl/Cmd+wheel or trackpad pinch zooms around the pointer.
Space+drag or middle mouse pans; R+drag rotates. The left rail fits, resets to
100%, rotates, flips and resets rotation. View changes never modify export pixels.
The document limit is 16,384 pixels per side; dense images also have a memory
admission limit. Image files and PNG exports are capped at 128 MiB. Streamed v2
projects have a 4 GiB total-file cap and also require enough local disk storage.
16-bit input and color-profile conversion remain later-phase work.

B selects brush, E eraser, I eyedropper, Alt temporarily samples a document pixel,
[ and ] resize the brush, and X swaps primary/secondary colors. Ctrl+Z undoes one
stroke, Ctrl+Shift+Z or Ctrl+Y redoes. Ctrl+Z during a stroke cancels it. Escape
cancels the current stroke; losing
pointer capture or window focus also cancels it. The right panel offers hard/soft
round, airbrush and pixel-pencil presets, HSV/hex/recent colors, dab settings,
both stabilizers, and editable pressure/tilt curves. On narrow windows, use
Brush settings to open the panel. Image brush tips and textures remain P9.

Every stroke uses a bounded temporary preview and one exact COW tile command.
Flow accumulates per dab; opacity caps the complete stroke. The inspector's History
button opens the history panel: click a state to jump, or configure memory/step
limits. Defaults are 256 MiB / 200 steps within the shared 768 MiB CPU admission
limit, including document and temporary buffers. Old retained states can be evicted
to make room. Zero disables retained undo; an edit larger than the history cap can
complete without an undo step. Resolution changes use commands and merge within a
single focus/pointer gesture; they change DPI metadata without resampling pixels.

The Layers panel adds, duplicates, renames, reorders and groups layers. Ctrl/Shift
selects several rows; new groups default to pass-through. Select a mask to paint
it, or Alt-click to view it. Click the layer name to return to its pixels. Layer
properties include blend mode, opacity, fill, visibility, clipping and locks.
Mask and merge operations are undoable; thumbnails update after a stroke.

M selects rectangle, Shift+M ellipse, L freehand lasso, Shift+L polygonal lasso,
W magic wand and G paint bucket. Shift adds, Alt subtracts and both intersect;
the toolbar also offers explicit modes, anti-aliasing, tolerance, contiguous
matching and all-layer sampling. Enter/double-click completes a polygon;
Escape cancels it. The Select menu includes all/none/inverse, feather, grow,
shrink, border, smooth and foreground color range. Q toggles Quick Mask: black
removes selected coverage, white or eraser adds it, and red marks excluded pixels.
Brushes, erasers and fills respect the selection, including partial coverage.

The Edit menu offers foreground/background Fill, Clear, Cut, Copy, Copy merged,
Paste and Paste in place. Standard Ctrl/Cmd+C/X/V shortcuts work; Ctrl/Cmd+Shift+V
pastes in place. Paste creates a raster layer. The internal clipboard preserves exact
straight samples and source position. System PNG clipboard access depends on
browser permissions; failures are visible and the internal copy remains usable.
External clipboard conversions may normalize transparent RGB. Active selections
are session state and are omitted from project/recovery files.

Save project preserves layers, masks, groups, straight RGBA8 samples, hidden RGB
and exact DPI in a versioned `.cgfoto` file. Layered/large saves use a streamed
writable file destination; small default single-raster projects can download in
the legacy format. Open project or drop the file to reopen with fresh undo
history. Project files store the document only, not history or view state.

Recovery saves changed, completed documents every 30 seconds and on tab hide when
idle. Tab-hide saves are best effort: startup asks to restore the last completed
snapshot. Recovery storage is local to this browser/profile/origin; tabs own
independent sessions and do not restore or overwrite active sessions. Up to three
validated generations remain per session; restored source archives remain until
Discard recovery removes them. Storage denial/quota/write failures are visible in
the Recovery section; Save recovery now retries a failed save. Keep project files
as independent copies because browser data can be cleared or evicted. Save/recovery
operations briefly pause editing and report their current operation.

## Prerequisites

- Node.js 24 and pnpm 12.11.2.
- Rust 1.99.1 (pinned by rust-toolchain.toml).
- On Windows: Visual Studio C++ x64/x86 tools, Windows SDK, and WebView2 Runtime.
  Rust procedural macros also require the native linker when building WASM.
- Chrome and Edge for hardware browser tests.
- Current browser/WebView2 WASM SIMD and Relaxed SIMD support for the engine.

The desktop runtime is separate from Microsoft Edge itself. On Windows, run
scripts/setup-webview2.ps1 to detect it and install Microsoft's signed Evergreen
bootstrapper only when it is missing. CI performs this check explicitly.

## Run

    pnpm install --frozen-lockfile
    pnpm setup:wasm
    pnpm dev

Open http://127.0.0.1:5173. Rust changes rebuild WASM and restart the worker through
Vite HMR. UI and pointer-event collection stay on the main thread; document/pixel
processing and GPU resources belong to one dedicated worker.

    pnpm build
    pnpm preview
    pnpm desktop:dev
    pnpm desktop:build
    pnpm desktop:smoke

The desktop build produces an unbundled Windows executable under target/release.
Installers, native file integration, and updating are P10 work.

## Verify

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p fotoeditor-wasm --target wasm32-unknown-unknown -- -D warnings
    cargo test --workspace
    pnpm typecheck
    pnpm lint
    pnpm format:check
    pnpm licenses:check
    pnpm test
    pnpm test:e2e
    pnpm bench:viewport
    pnpm bench:brush
    pnpm bench:history

Build before typechecking, E2E, or compiling the desktop shell: generated WASM
bindings and the web distribution are intentionally untracked.

Browser tests use installed Chrome and Edge by default. Windows CI sets
GPU_TEST_SOFTWARE=1 and installs Playwright Chromium. One project selects the
SwiftShader WebGPU adapter with a WARP software compositor; a second forces SwiftShader
for WebGL2. Tests verify the software adapter rather than silently accepting
another backend. Software results demonstrate correctness, not hardware performance.
Tests start their own strict-port preview on 4187 (override with E2E_PORT); they do
not reuse an unrelated preview. Hardware benchmarks use port 4188. Build once before
a suite and keep that distribution unchanged until its server exits.

E2E generates deterministic alpha, JPEG/WebP and 6000×4000 PNG fixtures with a
native Rust example in bench/results/fixtures. The hardware viewport benchmark
starts a local preview, checks the adapter, and measures three image opens plus
four-second pan, zoom and cross-mip zoom sessions on both installed browsers and
both backends. JSON and screenshots go to bench/results. It requires a built web
distribution and refuses a software adapter. Refinement keeps a cached preview
visible while sharpening after cold mip transitions; exact export uses CPU samples.

The brush benchmark uses the same deterministic 6000×4000 image fixture, a 300 px
soft brush at 100% view and 1800 px/s, with native protocol input at 120 Hz on each
installed browser/backend. Two four-second trials follow a priming dab. It reports frame and
sample-to-frame-acknowledgement percentiles, GPU identity and logical residency in
bench/results/p2-performance.json. It refuses software adapters. Acknowledgement
timing starts at the source sample that first changes a presented document pixel;
full-dab completion is reported separately. It does not include compositor scanout
or physical pen latency. Synthetic
pressure/tilt browser tests pass through Chromium's pen input path; no physical pen
is available, and the user explicitly deferred physical Windows Ink validation.

The history benchmark runs 200 separate soft dabs on the same 6000×4000 document,
checks history/CPU limits, measures two warmed project save/reopen cycles with
byte-identical hashes, and measures four flushed/readback-validated autosaves while
checking three retained generations. Its worker timings and UI observation timings
are separate. Evidence goes to bench/results/p3-history-performance.json.
Set PERF_PHASE=p3 for the brush/viewport benchmarks to retain P3 regression evidence
without replacing prior phase results.

The desktop smoke test uses a temporary profile and a loopback debugging port.
On disposable GitHub-hosted runners only, it temporarily sets an executable-scoped
HKLM browser-argument policy, then restores it. Elevated WebView2 hosts ignore
environment browser flags, as documented in
[Microsoft's WebView2 guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security#for-an-elevated-host-app-use-appropriate-override-flags).
Normal app launches do not expose a debugging port.

Use ?gpu=webgl2 to force WebGL2. The ?diagnostic=1 scene uploads a 2x2 texture from
Rust and displays red, green, blue, and white quadrants. Its diagnostic controls
exercise device-loss and panic recovery. ?diagnostic=1&fault=init tests initialization
failure; Retry creates a new canvas and worker.
Additional diagnostic controls inject quota/partial-write failures or pause an
uncommitted generation for renderer-kill tests. They are hidden in normal use.

Production builds measure optimized WASM and enforce a maximum of 1,500,000
gzipped bytes. The report is written to bench/results/wasm-size.json.

Read docs/DEPLOYMENT.md for static hosting and docs/DECISIONS.md for architectural
decisions. docs/PLAN.md tracks the reviewed roadmap; docs/PROGRESS.md records verification.
