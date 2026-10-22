# CashGear FotoEditor

A focused photo editing and painting workspace. Open PNG, JPEG and WebP images,
create transparent canvases, paint with procedural brushes, and export lossless
RGBA8 PNG. The worker-owned Rust engine processes sparse tiles and displays them
through WebGPU or WebGL2.

Use New canvas or Open image, or drop an image into the viewport. Wheel/two-finger
gestures scroll; Ctrl/Cmd+wheel or trackpad pinch zooms around the pointer.
Space+drag or middle mouse pans; R+drag rotates. The left rail fits, resets to
100%, rotates, flips and resets rotation. View changes never modify export pixels.
The document limit is 16,384 pixels per side; dense images also have a memory
admission limit. PNG/JPEG/WebP files and PNG exports are capped at 128 MiB.
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
Flow accumulates per dab; opacity caps the complete stroke. The 768 MiB CPU
admission limit includes document, history and stroke scratch buffers. P2 rejects
edits that exceed it; the history panel, eviction, project save and recovery are P3.

## Prerequisites

- Node.js 24 and pnpm 12.11.2.
- Rust 1.99.1 (pinned by rust-toolchain.toml).
- On Windows: Visual Studio C++ x64/x86 tools, Windows SDK, and WebView2 Runtime.
  Rust procedural macros also require the native linker when building WASM.
- Chrome and Edge for hardware browser tests.

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

Build before typechecking, E2E, or compiling the desktop shell: generated WASM
bindings and the web distribution are intentionally untracked.

Browser tests use installed Chrome and Edge by default. Windows CI sets
GPU_TEST_SOFTWARE=1 and installs Playwright Chromium. One project selects the
SwiftShader WebGPU adapter with a WARP software compositor; a second forces SwiftShader
for WebGL2. Tests verify the software adapter rather than silently accepting
another backend. Software results demonstrate correctness, not hardware performance.

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

Production builds measure optimized WASM and enforce a maximum of 1,500,000
gzipped bytes. The report is written to bench/results/wasm-size.json.

Read docs/DEPLOYMENT.md for static hosting and docs/DECISIONS.md for architectural
decisions. docs/PLAN.md tracks the reviewed roadmap; docs/PROGRESS.md records verification.
