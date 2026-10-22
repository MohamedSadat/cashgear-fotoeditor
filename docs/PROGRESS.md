# Progress

## P0 — Local acceptance passed; awaiting remote CI

Shipped:

- Seven Rust crates, a React/Vite workspace, and a minimal Tauri/WebView2 shell.
- Worker-owned wgpu renderer with actual-adapter detection, WebGL2 fallback,
  physical-pixel sizing, frame backpressure, and fresh-worker recovery.
- Clear canvas, Rust-uploaded texture diagnostics, fault injection, and dark shell.
- Static isolation headers, reproducible toolchain setup, and GitHub Actions.
- Reviewed roadmap decisions and a checked inventory of 431 dependencies.

Local verification on 2026-10-22:

| Check                                       | Result                                                   |
| ------------------------------------------- | -------------------------------------------------------- |
| Rust formatting and native/WASM clippy      | Passed                                                   |
| Cargo workspace tests                       | Passed; diagnostic WGSL validated                        |
| TypeScript, ESLint, Prettier                | Passed                                                   |
| Vitest                                      | 4 passed                                                 |
| Chrome 156.0.8078.47 and Edge 155.0.4283.62 | 24 browser tests passed                                  |
| Software Chromium                           | 14 tests passed; SwiftShader explicitly asserted         |
| Windows/WebView2 155.0.4283.62              | Both backends render; isolation confirmed                |
| Development Rust watch/HMR                  | Rebuild and worker replacement passed                    |
| License inventory                           | 431 dependency expressions approved                      |
| Release WASM                                | 1,912,638 bytes; gzip level 9: 782,613 / 1,500,000 bytes |

Hardware browser and desktop checks used Windows 10 with NVIDIA GeForce GTX 1650
(driver 27.21.14.5751). WebGL2 identifies the NVIDIA adapter; WebGPU intentionally
does not expose a descriptive adapter name through wgpu on this browser. Software
tests separately verify Google's SwiftShader WebGPU vendor and the WebGL2 driver.

Known limitations: P0 opens no image and has no editing tools. Installer/updater and
native file integration remain P10 work. Windows 11 and other hardware are not
claimed as tested. Static hosting is prepared; no public deployment was requested.

The p0-scaffold completion tag will be created only after GitHub Actions passes.

No later editing phase has started.

Suggested next phase: P1 — Tiles, viewport, open and PNG export. It establishes
straight-alpha samples and residency accounting before pixel edits begin. The
four outstanding product questions are recorded in PLAN.md.
