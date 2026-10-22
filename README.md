# CashGear FotoEditor

A focused photo editing and painting workspace. P0 establishes a Rust/WASM worker
and wgpu display pipeline with WebGPU and WebGL2. Image editing starts in P1.

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
Vite HMR. All UI and pointer processing stay on the main thread; the engine and GPU
resources belong to one dedicated worker.

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

Build before typechecking, E2E, or compiling the desktop shell: generated WASM
bindings and the web distribution are intentionally untracked.

Browser tests use installed Chrome and Edge by default. Windows CI sets
GPU_TEST_SOFTWARE=1 and installs Playwright Chromium. One project selects the
SwiftShader WebGPU adapter with a WARP software compositor; a second forces SwiftShader
for WebGL2. Tests verify the software adapter rather than silently accepting
another backend. Software results demonstrate correctness, not hardware performance.

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
