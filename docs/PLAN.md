# Reviewed implementation roadmap

Reference: Raster Editor - Implementation Plan, dated 2026-10-21, 21 pages,
provided by the user. The source PDF remains unchanged. Its instructions were
reference material; user choices and the approved P0 plan establish repository rules.

| Phase | Outcome                                                                       | Tag          |
| ----- | ----------------------------------------------------------------------------- | ------------ |
| P0    | Workspace, worker/WASM, GPU fallback, platform proof, CI                      | p0-scaffold  |
| P1    | Straight-alpha sparse tiles, residency model, viewport, image open/PNG export | p1-viewport  |
| P2    | Commands/basic undo foundation, brushes and color                             | p2-brush     |
| P3    | History UI/budgets, versioned project save/reopen, crash recovery             | p3-history   |
| P4    | Layers, masks, groups, CPU/GPU compositing                                    | p4-layers    |
| P5    | Selections, fills, clipboard                                                  | p5-select    |
| P6    | Adjustments and filters in independently accepted milestones                  | p6-filters   |
| P7    | Transforms, crop, resampling, guides                                          | p7-transform |
| P8    | Extended formats, export, validated PSD interoperability                      | p8-files     |
| P9    | Retouch and advanced painting in independently accepted milestones            | p9-retouch   |
| P10   | Windows packaging, file integration, updater                                  | p10-desktop  |
| P11   | 16-bit processing with enforced memory budgets/paging as needed               | p11-16bit    |
| P12   | Threading, SIMD, paging expansion, accessibility, release                     | v1.0.0       |

The original roadmap's detailed phase questions remain prompts for the next phase,
not permission to implement all phases during P0. Record answers in DECISIONS.md.

## P0 acceptance

- Seven Rust crates plus web and minimal desktop workspaces.
- Worker owns WASM renderer and OffscreenCanvas; typed init/resize/frame/dispose API.
- WebGPU detection tests an actual adapter; device-request failure can fall back
  before a canvas context is acquired. Forced WebGL2 works.
- Native-size canvas rendering, DPR/resize handling, serialized frames, paused zero-size viewport.
- Visible failures and fresh-worker/fresh-canvas recovery.
- Clear-canvas and Rust-uploaded diagnostic texture verified on both backends.
- COOP/COEP and correct WASM delivery in production preview/static deployment.
- Rust/JS checks, hardware Chrome/Edge tests, software CI, WebView2 launch.
- Optimized release WASM at most 1,500,000 bytes gzipped.
- Dependency inventory, reproducible setup, CI, decisions and progress records.
- Commit to main; tag p0-scaffold only after remote CI and all P0 acceptance pass.

## P1 acceptance

User choices are recorded in DECISIONS.md; all four phase questions are answered.

- Generic Pixel/Rgba8/Gray8, sparse COW tiles, dirty tracking and exact straight alpha.
- One raster document, selected new-canvas presets, 16,384 maximum side validation.
- Worker Rust PNG/JPEG/WebP opening including drag/drop and lossless RGBA8 PNG export.
- Early CPU/GPU/history/temporary accounting, bounded allocations and evictable mirrors.
- Visible atlas tiles, minified filtered mips, nearest at >=100%, grid at >=800%.
- Zoom 1%–3200%, pointer anchoring, pan, rotate, flip, fit and actual pixels.
- Status reports zoom, cursor document pixel, dimensions and GPU backend.
- Transform round-trips within 1e-8 document pixels, at all tested zooms/rotations/flips.
- Measure 6000×4000 PNG opening (<1.5 seconds) and 60 Hz pan/zoom on specified
  hardware/browser/dataset with cold/warm state and frame percentiles.
- Full required Rust/JS checks, production size, Chrome/Edge tests on WebGPU/WebGL2,
  and remote CI green before pushing p1-viewport. Software results prove function only.

## P1 product questions (answered 2026-10-22)

1. Largest required canvas and whether 6000x4000 / 16384 maximum side are appropriate?
2. New-document size and DPI presets?
3. Transparency checkerboard and canvas background preferences?
4. Wheel zoom versus scroll, and trackpad pinch behavior?
