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

## P2 acceptance

The detailed source requirements are on printed pages 11–12. The approved
correction moves general commands and basic undo ahead of painting.

- General apply/revert commands with COW tile snapshots, atomic admission,
  exact undo/redo and unique document/history/temporary residency accounting.
- One command per stroke; Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y. History panel, configurable
  budgets/eviction, project files and recovery remain P3.
- Round dabs: size, hardness, opacity, flow, spacing, roundness and angle.
- Editable pressure and tilt curves for size and opacity; coalesced pen samples,
  path interpolation, stabilizer and strength control.
- Stroke buffer: flow accumulates per dab, opacity caps the complete stroke.
- Eraser, eyedropper, primary/secondary colors, swapping, HSV square, hue slider,
  hex input and recent colors. Hard round, soft round, airbrush and pixel pencil.
- True-size brush outline and dirty-tile live preview through the frame pump.
- Measure a 300 px soft brush on a 6000×4000 document at 60 fps and pointer-to-pixel
  latency under one frame; no gaps on fast flicks. Pressure on Windows Ink in
  Chrome/Edge requires actual available pen hardware and explicit evidence.
- Full required checks, both installed browsers/backends, desktop verification,
  production size, measured hardware results and remote CI green before p2-brush.

## P2 product questions (answered 2026-10-22)

1. No pen hardware available. User approved synthetic coverage for P2 completion;
   physical Windows Ink validation is explicitly deferred and remains unverified.
2. Photoshop shortcuts B/E/I/[ / ]/X and Alt eyedropper confirmed.
3. Both pulled-string and weighted-average stabilizers.
4. Image tips/textures in P9; P2 uses procedural tips.

## P3 acceptance

The detailed source requirements are on printed pages 12–13. General commands,
COW snapshots, exact stroke undo/redo and shortcuts already shipped in P2.

- History panel with click-to-jump, configurable memory/step limits and oldest
  history eviction within the existing 768 MiB total CPU admission model.
- Merge applicable consecutive document property commands; keep each stroke separate.
- Versioned project save/reopen preserving exact straight RGBA8, sparse tiles and DPI.
  Reject corrupt/unsupported input without replacing the live document.
- Recoverable OPFS generations, retaining prior valid snapshots, explicit flush and
  readback validation, quota/persistence failure handling and coordinated writers.
- Main-thread persistent-storage request; visible recovery and storage status.
- Configured autosave interval/tab-hide behavior and startup restore flow.
- 500 property-tested command sequences return byte-identical document hashes after
  undo-all; 200 brush strokes stay within the configured and total memory budgets.
- Killing a tab restores its last completed autosave; interrupting a write or corrupting
  the latest snapshot leaves a prior valid generation recoverable.
- Full required checks, Chrome/Edge on WebGPU/WebGL2, appropriate Windows/WebView2
  verification, measured performance, and remote CI/deployment green before p3-history.

## P3 product questions (answered 2026-10-23)

1. 256 MiB history / 200 steps, configurable, within the shared 768 MiB CPU limit.
2. Autosave every 30 seconds when changed, plus best-effort tab hide.
3. Ask “Restore last session?” on startup.
4. Document-only project/recovery files; reopen with fresh undo history.

## P4 acceptance

The detailed source requirements are on printed page 13. All four product
questions and the dense-workload/file-cap conflict were answered on 2026-10-23;
see DECISIONS.md and P4-DESIGN.md.

- Layers panel: add/delete/duplicate/rename, drag reorder, multi-select and groups.
- Opacity, fill, 16 blend modes, visibility and pixel/alpha/position/all locks.
- Masks: add, paint, disable, apply, delete, and Alt-click mask view; clipping below.
- Merge down, merge visible and flatten. All document mutations use exact COW
  undo/redo commands, including layer, group and mask operations.
- Independent Rust CPU reference and portable WGSL GPU compositors. Golden
  comparisons within ±1/channel for every mode, with masks and groups.
- Worker-generated low-resolution thumbnails, refreshed after each stroke.
- Bounded CPU residency with immutable paged tile generations, shared history
  accounting, bounded GPU composite caches, and visible storage failures.
- Streamed versioned layer/mask/group project and recovery formats, 4 GiB project
  cap, strict validation, atomic candidate commits, v1 single-raster read support,
  and recoverable coordinated generations. Images retain their 128 MiB cap.
- Fifty distinct dense 4000×3000 layers pan at 60 fps on measured hardware. Record
  dataset, storage, memory, cold/warm conditions and percentile frame timings.
- Full required checks, installed Chrome/Edge and both backends, Windows/WebView2,
  size and license gates, remote CI and deployment before tagging p4-layers.
- Stop before P5 selections/fill/clipboard.

## P4 product questions (answered 2026-10-23)

1. sRGB blending.
2. Pass-through groups by default.
3. Fifty dense layers; paging and 4 GiB streamed projects approved within unchanged
   CPU/GPU budgets and unchanged image-file limits.
4. Thumbnails after the stroke ends.

## P5 acceptance

The detailed source requirements are on printed page 14 of the original roadmap.
All four product choices were answered on 2026-10-24; see DECISIONS.md.

- Gray8 tiled selections with new/add/subtract/intersect and Shift/Alt modifiers.
- Rectangle, ellipse, freehand/polygonal lasso, magic wand with tolerance,
  contiguous and sample-all-layers controls; anti-aliasing defaults on.
- Select all/none/invert/feather/grow/shrink/border/smooth/color range.
- GPU mask-edge marching ants and editable red-overlay Quick Mask.
- Brush, eraser and fill clip through selection coverage. Establish a reusable
  Rust clipping contract for P6 filters without starting P6.
- Paint bucket and Edit Fill; exact command undo/redo for every mutation.
- Copy/cut/paste/paste in place, with new-layer paste, internal exact samples and
  system image clipboard access through the Clipboard API.
- Magic wand on a 6000×4000 photo completes in under 200 ms on measured hardware;
  specify tolerance, connected extent, sampling source and cold/warm conditions.
- Painting does not change any sample outside the selection. Compare feathering
  visually with an identifiable Photoshop reference and record radius/fixture.
- Preserve paging, CPU/GPU budgets, straight alpha/hidden RGB, file caps and
  atomic, bounded/cancellable candidate jobs. Selection buffers and clipboard
  samples must be included in residency accounting.
- Full repository checks, installed Chrome/Edge on both APIs, Windows/WebView2,
  size/license gates, hardware measurements, remote CI/deployment green, then
  publish p5-select. Do not declare the unavailable feather comparison passed.
- Named selection channels are optional in the source roadmap and remain a
  follow-up. Stop before P6.

## P5 product questions (answered 2026-10-24)

1. Quick Mask in v1: yes.
2. Anti-aliased edges by default: yes.
3. AI Select Subject outside v1: confirmed.
4. Paste behavior: always a new layer.

## P6 acceptance and milestone boundaries

Source: complete P6 section on original roadmap printed pages 14–15. The four
product questions were answered on 2026-10-24; see DECISIONS.md and P6-DESIGN.md.
The user explicitly authorized starting while the P5 feather comparison is open.
M1 was accepted on 2026-10-24; see P6-M1-REPORT.md. M2 implementation and local
acceptance are complete on 2026-10-24; see P6-M2-REPORT.md. Its task completion
records the final pushed CI/deployment gate. M3 implementation and local
acceptance are complete on 2026-10-25; see P6-M3-REPORT.md. Its task completion
records the final pushed CI/deployment gate. M4 implementation and local acceptance
are complete on 2026-10-25; see P6-M4-REPORT.md. The annotated p6-filters tag
certifies final-head CI/deployment, inspected artifacts and full deployed hardware
regression. P7 started on 2026-10-25 at the user's explicit request.

1. M1: brightness/contrast, levels, curves, exposure and hue/saturation as bounded,
   selection-clipped destructive commands; exact undo/redo, locks, paging,
   cancellation and refusal; an always-available exact active-raster histogram.
2. M2: those five adjustments as editable adjustment layers, CPU/GPU parity,
   masks/groups/blends/project/recovery integration, and GPU viewport live preview.
3. M3: Gaussian blur, box blur, unsharp mask, sharpen, median, tiled neighbor
   padding, selection clipping, live preview and Fade Last Filter (opacity/blend).
4. M4: vibrance, color balance, black & white, invert, threshold, posterize and
   gradient map (destructive and layers); motion blur, high pass, add noise,
   emboss and pixelate. Complete phase-wide validation and report, then stop.

- Gaussian blur radius 20 on 6000×4000 must apply in under 2 seconds on measured
  single-threaded hardware. Preview must update within 100 ms of slider movement.
- Golden tests must show no tile seams; adjustment layers must match destructive
  results within ±1/channel. M1-M4 implement and verify these processing contracts.
- Final-worker measurements: all 24 Gaussian calls pass (maximum 1.55143 s), and
  all 2,240 warm control events pass (maximum 81.21 ms). First-use preparation,
  slower additional filters and supplemental legacy performance limits are
  reported separately in P6-M4-REPORT.md.
- Preserve straight-alpha/hidden RGB, shared 768 MiB CPU / 128 MiB GPU budgets,
  paging, 128 MiB image limits and 4 GiB streamed project cap. No smart filters.
- Every milestone runs applicable repository checks, installed Chrome/Edge on
  WebGPU/WebGL2, Windows/WebView2, size/license checks and remote CI/deployment.
  Publish p6-filters only after every P6 milestone and acceptance criterion passes.
  M1 is not phase completion. Stop before P7.

## P7 acceptance

Source: the complete P7 section on printed pages 15-16 of the original roadmap,
read as text and rendered pages. P6 is complete at `5a22595`, with the remotely
verified annotated `p6-filters` tag. P7 starts on main without agents/worktrees.
See P7-DESIGN.md for implementation and evidence contracts.

- Move tool with optional layer auto-select; arrow nudge by one pixel, Shift by ten.
- Free transform of a raster layer or selected pixels: scale, rotate, skew,
  distort and perspective, visible handles, Shift proportions, Enter commit and
  Escape cancel. An uncommitted session always samples its immutable source.
- Live GPU transform preview; one final CPU resample on commit. Nearest, bilinear,
  bicubic and Lanczos3; fast_image_resize for axis-aligned scaling and an own
  arbitrary-matrix sampler. Changing interpolation never edits document samples.
- Image size and anchored canvas size dialogs; rotate canvas by 90/180/arbitrary
  degrees and flip. Include masks, selections, nested layers and exact undo/redo.
- Non-destructive crop with ratio presets, straighten and rule-of-thirds overlay.
  Retain outside pixels through paging, project save/reopen and recovery.
- Rulers, editable guides and snapping in document coordinates, including rotated,
  flipped and zoomed views. Persist document guide/crop metadata as appropriate.
- A 10-degree rotation followed by its inverse within one uncommitted session
  stays visually clean, with at most one resample on commit. Two committed
  destructive transforms each resample, as already decided in DECISIONS.md.
- A 50% Lanczos3 downscale matches an identifiable independent reference within
  two values per channel. Cover canvas/tile boundaries, transparent edges and
  masks; identity/integer translation/right-angle rotations preserve exact samples.
- All crop, move, transform, size, rotation, flip and guide mutations use commands
  with exact undo/redo. Cancellation, invalid/singular matrices, locks, paging
  failures and budget refusal leave document/history/redo intact.
- Preserve straight alpha/hidden RGB, paged generations, 768 MiB total logical CPU
  and 128 MiB GPU budgets, 128 MiB image cap and 4 GiB streamed project cap. Include
  retained transform sources, candidates, scratch and GPU previews in accounting.
- Measure meaningful 6000x4000 hardware Apply and preview workloads with browser,
  API, dataset, cold/warm conditions and percentile timings. The source specifies
  no separate P7 millisecond target; do not invent a passed performance threshold.
- Run formatting, native/WASM clippy, Cargo tests, TypeScript, ESLint, Vitest,
  production build/size and license checks; installed Chrome/Edge WebGPU/WebGL2,
  Windows/WebView2 on both APIs, exact-final-head remote CI/deployment and artifact
  inspection for failures/skips/retries. Tag/push `p7-transform` only after every
  criterion passes. Report shipped/deferred work, measurements and proposed P8;
  stop before starting P8.
- P5's identifiable Photoshop feather comparison remains pending without
  `p5-select`. Physical Windows Ink validation retains its user-approved deferral.

## P7 product questions (answered 2026-10-25)

1. Lanczos3 default; the other three interpolation modes remain available.
2. Warp and puppet warp after v1; affine/projective free transform is in P7.
3. Hide outside pixels non-destructively, including across project/recovery saves.
4. Content-aware scale outside v1.

## P8 acceptance

Implementation, local verification and final normal-document hardware
measurements are recorded in [P8-REPORT.md](P8-REPORT.md). The annotated
`p8-files` tag certifies exact-final-head remote CI/deployment and artifact
inspection. The user-accepted save-speed exception below remains a P12 target.
Stop before P9.

Source: the complete P8 section on printed pages 16-17 of the original roadmap,
read as text and rendered pages on 2026-10-27. The source PDF remains unchanged.
P7 is complete at `86043ed`, certified by the remotely verified `p7-transform`
annotation. P8 starts directly on main, without worktrees or agents.

- Full document save/reopen with an identical canonical document hash, preserving
  exact straight samples, hidden RGB, layer tree/properties, masks, adjustments,
  retained outside-crop generations, DPI and guides. Fresh history on reopen and
  session-only selections remain existing decisions.
- The user selected ZIP with versioned `manifest.json`, LZ4 tile blobs,
  `thumbnail.png` and `composite.png`. Keep `.cgfoto` and legacy reading. Do not
  assume this proposal is already satisfied by the current uncompressed stream.
- Explicit schema compatibility/migrations and adversarial/fuzz reader coverage:
  invalid lengths, directories, checksums, versions, references, decompression
  limits and malformed metadata must never publish a partial candidate.
- Chromium File System Access open/save, download fallback where available, and
  recent files with visible permission/missing-file behavior. Preserve bounded
  streaming for large projects; no whole-file 4 GiB allocation.
- Export dialog: PNG, JPEG quality, WebP quality/lossless, encoded size preview,
  full document/active layer/selection sources. Export only the visible canvas
  window. Lossy formats and JPEG's alpha matte must have explicit behavior.
- Convert embedded ICC profiles to sRGB on import using a validated Rust color
  management path. Embed sRGB ICC by default, as selected by the user. No silent
  unsupported-profile reinterpretation.
- PSD raster import: correct layer order, opacity, supported blends, visibility
  and editable masks. Select/extend/replace the proposed `psd` crate based on its
  actual mask, bounds, channel and malformed-file behavior. Unsupported features
  must be reported explicitly. The user selected import only; PSD export is
  outside P8. No 16-bit downconversion hidden behind RGBA8 APIs.
- Validate Photoshop-created files using an identifiable corpus with provenance,
  expected metadata, layer/channel/mask samples and actual reference outputs.
  Synthetic/self-roundtrip fixtures alone cannot establish Photoshop compatibility.
- Measure a real 20-layer 6000x4000 project save against the original under-three-
  seconds target. Specify distinct dense samples, file size/hash, storage, browser,
  API, cold/warm conditions, timing boundaries, percentiles and logical residency.
  Sparse/shared duplicates do not substitute for this workload. Record misses.
  On 2026-10-27 the user explicitly accepted the measured P8 save-speed limit
  and deferred the under-three-second target to P12; continue full measurements
  and report the miss. Other acceptance requirements remain unchanged.
- Preserve 16,384-pixel retained/visible sides, paging, shared 768 MiB CPU and
  128 MiB GPU budgets, 128 MiB image limits and the 4 GiB streamed project cap.
  Count codec, conversion, compression, thumbnail/composite, file/bridge and
  candidate buffers. Candidate work yields/cancels and commits atomically.
- Formatting, native/WASM clippy, Cargo tests, TypeScript, ESLint, Vitest,
  production size/licenses, installed Chrome/Edge on WebGPU/WebGL2, Windows/
  WebView2, measured hardware results and exact-final-head remote CI/deployment
  are required. Inspect artifacts for failed/skipped/retried cases. Publish
  `p8-files` only after all applicable acceptance passes; report and stop before P9.
- P8's gzip WASM limit is 2,000,000 bytes, explicitly approved by the user on
  2026-10-27. Earlier phase size reports retain their historical 1,500,000-byte gate.
- Carry forward P5's pending identifiable Photoshop feather comparison without
  `p5-select`, the approved physical Ink deferral and the reported P6/P7 limits.

## P8 product questions (answered 2026-10-27)

1. PSD import only.
2. BMP, GIF first frame and 8-bit TIFF; HEIC not selected.
3. Embed sRGB ICC by default on exports.
4. Implement ZIP/LZ4 projects, preserving `.cgfoto` and backward reading.
5. No local Photoshop or fixtures. Investigate public corpus; actual validation
   remains an acceptance gate.

The existing `.cgfoto` extension is retained rather than asking again about an
already shipped product name. See P8-DESIGN.md for the current investigation.

## P9 acceptance and milestone boundaries

M1 is accepted at `c630690977e1a3ca3a5558f2dd0d3ee78808540f`; its final
certification JSON records immutable remote evidence. M2 implementation, local
hardware verification and measurements are complete on 2026-10-28; see
P9-M2-REPORT.md. Its completion message/final certification JSON record the
exact-head remote CI/deployment, downloaded artifacts and hosted verification.
M2 does not complete P9. Stop before M3 until explicitly requested.

Source: complete original roadmap P9 section on printed page 17, extracted and
visually inspected on 2026-10-27. Source SHA-256 is recorded in P9-DESIGN.md.
The user answered all four questions: clone first, CashGear presets only, mirror
and radial symmetry, editable multi-stop gradients. See DECISIONS.md.

- M1: aligned/non-aligned Clone Stamp, current/all-layer sampling, 200 px brush
  at 60 fps on measured hardware, selection clipping and exact command undo.
  The user explicitly accepted the measured M1 speeds on 2026-10-27: current
  layer 59.79-60.02 presentation fps; all layers 25.89-32.61 fps. The all-layer
  60 fps miss is deferred to P12, not reported as passed. Other gates remain.
- M2: Poisson healing brush and nearby patch-search spot healing. Healing a
  blemish on an identifiable reference portrait must leave no visible seam.
- M3: own PatchMatch content-aware fill without GPL code.
- M4: smudge, blur/sharpen, dodge/burn, sponge; linear/radial/angle/reflected/
  diamond gradients with editor and reusable multi-stop presets.
- M5: image tips, scatter, jitter, texture, CashGear preset import/export, mirror
  and radial symmetry; complete phase-wide validation before `p9-retouch`.
- Every tool respects selection and exact undo; cancellation/refusal is atomic.
  Preserve straight samples/hidden RGB, locks, masks, retained crop pixels,
  guides/DPI, legacy reading, paging, shared CPU/GPU budgets and file/size limits.
- Each milestone runs applicable full verification, browser hardware APIs,
  WebView2, size/licenses, exact-head remote CI/deployment and artifact inspection.
  Detailed M1 criteria are recorded before implementation in P9-DESIGN.md.
- Tag/push `p9-retouch` only after all milestones and acceptance gates pass.
  Report shipped/deferred work, hardware measurements and proposed P10, then stop.
  Existing P5/Ink/P6/P7 and accepted P8/P12 limits retain their status.
