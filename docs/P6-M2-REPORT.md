# P6 M2 — Adjustment layers and viewport preview

Implementation resumed on 2026-10-24 after the user's instruction to continue
from accepted M1. Work stays on main, without agents or a worktree. P6 remains
in progress; M3 and M4 are outside this milestone. The implementation and local
acceptance below are complete. Milestone completion also requires green CI and
deployment for the pushed implementation; the task's completion message records
their immutable run links. No P6 completion tag is authorized at M2.

## Delivered behavior

- Brightness/contrast, levels, master RGB curves, exposure and hue/saturation
  work as editable adjustment layers and as the existing destructive commands.
  Controls can create a layer above the active node or edit its saved settings.
- Insertion/editing, duplication, movement, visibility, masks, groups, clipping,
  opacity, fill and all 16 blend modes use the existing layer/command model.
  Metadata undo is exact. Initial selection coverage becomes the editable layer
  mask, retaining sparse defaults and tile sharing. Masks can be painted outside
  the original selection after deselecting, disabled or deleted. Layer merge
  controls rasterize results; adjustment masks remain editable independently.
- Schema-3 manifests persist settings and sparse mask defaults inside the
  bounded v2 stream framing. Schema-2 raster/group/mask files and legacy v1
  single-raster files remain readable. Unknown schemas, invalid settings and
  corrupt candidates are rejected before replacing the live document.
- Live viewport preview is opt-in. Changing its controls reuses visible sources;
  confirmation applies the full-resolution selection or one layer command.
  Preview creates no undo/recovery mutation. Export and autosave use committed
  content. Cancelling, changing document/view/target, and replacing the worker
  release its temporary state.
- The histogram remains available for the committed active raster, weighted by
  stored alpha before masks and layer effects. Select a raster node to inspect it.

## Processing and resource contract

All document sample processing remains Rust. The core crate now owns shared
adjustment definitions, re-exported by filters, avoiding a dependency cycle.
Independent float64 CPU composition and float32 WGSL use the same LUT/HSL
specification. Adjustment blend inputs are byte-quantized at the step boundary
to stabilize endpoint branches such as Color Burn. Inherited alpha is preserved.
Authoritative straight-alpha samples and fully transparent hidden RGB are retained.

The preview cache uses Rust nearest samples at visible pixel centers, bounded to
2048 pixels per side. It pins one authoritative source tile per step and yields
between preparation slices. A six MiB scratch reservation plus unique cached
generations is included in logical CPU residency. GPU composition follows the
full layer plan over screen tiles and presents premultiplied results without
JavaScript image readback. Initial readiness waits for the GPU queue fence;
coalesced later settings reuse these resources. A cancellable wait and UI queue
ownership prevent an obsolete request from blocking a replacement worker.

The unchanged limits are 768 MiB logical CPU, 128 MiB GPU, 128 MiB image files
and 4 GiB streamed projects. These account for buffers, not browser/allocator/
driver overhead. No external dependency or license was added.

Preview is an interactive display sample. Its downsampling can differ from the
committed full-resolution image at fitted zoom. Correctness comparisons at actual
pixels cover layered/masked output. The performance workload below is one opaque
24 MP raster; it does not establish latency for every layer count or viewport.

## Verification

Local results:

- All 95 Rust tests, native/WASM clippy, Rust/JS formatting, TypeScript, ESLint,
  all 16 Vitest cases, and the 450-expression license inventory pass.
- The full installed Chrome/Edge hardware regression passes 184 cases with no
  failures, skips or retries. Final shutdown/ownership/grid follow-ups pass all
  32 feature cases, explicitly checking each requested backend.
- The full software regression passes 134 cases, followed by all three added
  high-zoom grid cases. These are functional SwiftShader/WARP checks. The final
  CI suite contains all 137 cases together.
- Both hardware Windows/WebView2 APIs build, launch and pass the expanded smoke,
  including adjustment metadata, preview, exact undo and streamed persistence.
- Release WASM is 3,382,552 bytes / gzip9 1,279,503 bytes, under 1,500,000.
  The measured distribution's SHA-256 is
  `e5124e9f00fa1c0a6dc40152c93217a8c30ef570789a920da964d58987d0d95e`.

The earlier adjustment-layer foundation at `5705cf7` passes
[CI 40105107961](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40105107961)
and [deployment 40105108054](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40105108054).
The source-cache foundation is `2b537b6`, engine integration `398cc36`, and UI
integration `683f863`. Final viewport integration has its own required
CI/deployment gate. Its immutable run links are named in the task's completion
message only after both workflows pass.

Numerical coverage includes five adjustments × 16 blend modes × four mask,
clipping and isolated/pass-through group scenes: 320 complete tile comparisons
per installed browser/API, with at most one channel value of error. Rust tests
also compare adjustment layers with destructive math across alpha 0/1/128/255,
partial masks and odd tile boundaries; check metadata undo/no-ops/locks; and
exercise cold preview generations, geometry, persistence and schema rejection.

Browser scenarios cover painting sparse adjustment masks, settings editing,
exact undo, project roundtrip, actual renderer crash/recovery, GPU retry,
preview cancellation/refusal, held GPU waits, latest-setting coalescing and
preview beneath blended upper layers. The desktop smoke covers preview,
adjustment metadata, undo and streamed persistence alongside existing features.

## Measured hardware preview

Final artifact measurements separate source/GPU initialization from warm slider
latency. Evidence is written to
`bench/results/p6-m2-preview-performance.json` by `pnpm bench:adjustment-preview`.

The fixed workload is the existing 6000×4000 NASA/JPL-Caltech/IPGP photo, SHA-256
`603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`, cropped in
Rust without resampling from the official 6002×4000 JPEG. Source/credit:
[NASA wind and thermal shield](https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/).
Hardware is the existing Xeon E5-2680 v3 / GTX 1650 / 32 GB Windows 10 machine.
Chrome and Edge run at 1280×850, DPR 1, with an 878×494 preview cache. The scripts
verify the requested API and hardware adapter; software results are reported as
functional verification only.

Slider trials run before the API loops, so no earlier benchmark API calls warm
the first control. Each Exposure/HSL slider has 20 DOM input events, measured
through completed React state and the following UI frame. Five adjustment types
then have one first and ten warm API/presentation trials using the cached source.
Logical residency is sampled from the worker's accounted categories.

| Browser / API | Exposure slider p95 | HSL slider p95 | Slowest of 40 events | First ready, source + GPU |
| ------------- | ------------------: | -------------: | -------------------: | ------------------------: |
| Chrome WebGPU |            30.67 ms |       30.83 ms |             31.04 ms |                    388 ms |
| Chrome WebGL2 |            31.25 ms |       31.32 ms |             34.97 ms |                  1,027 ms |
| Edge WebGPU   |            31.48 ms |       31.91 ms |             33.64 ms |                    369 ms |
| Edge WebGL2   |            31.49 ms |       31.66 ms |             33.89 ms |                    955 ms |

All 160 recorded slider events are under 100 ms after preparation. Warm API
return p95 across all five controls is at most 20.81 ms. Sampled accounted
residency peaks at 104.1 MiB CPU and 22.49 MiB GPU for this workload. Initial
readiness above sums the recorded worker preparation and first render intervals;
it includes initial GPU work and is separate from warm interaction latency.

The existing generated 6000×4000 RGBA8 navigation workload measures
59.997–60.000 fps, worst presented-period p95/p99 16.73/16.77 ms, first opens
1.11–1.23 s, subsequent opens 0.93–1.06 s, and post-stop refinement 42.4–45.2 ms.
The 300 px soft brush, 10% spacing, 120 Hz native protocol mouse input at
1800 px/s measures 59.55–60.26 fps across two four-second trials per API/browser
after a priming dab. Worst pointer-to-presentation acknowledgement p95/p99 is
16.29/17.98 ms; worst frame p99 is 26.78 ms. Acknowledgement excludes compositor/
scanout and physical pen latency. Raw results are
`p6-m2-viewport-performance.json` and `p6-m2-brush-performance.json`.

## Corrections found during verification

- The GPU/CPU comparison found a Color Burn/HSL endpoint branch discrepancy;
  byte-exact adjustment blend inputs corrected it without loosening tolerance.
- Early startup measurements exposed a delayed WebGPU initialization spike.
  Initial readiness now includes a queue fence. Raw earlier measurements remain
  available alongside the final evidence.
- Fast worker startup/retry exposed null UI snapshots in new tests. Their helper
  now waits for the first published snapshot; command-returned history is used
  for the immutable baseline. The corrected full and final targeted runs pass.
- WebView2 exposed preview cleanup running while the panel did not own a preview.
  Cleanup now follows UI ownership. Queue generations and explicit GPU-wait
  cancellation cover replacement/cancellation during asynchronous initialization.

## Deferred work and next milestone

M3 is proposed next: Gaussian blur, box blur, unsharp mask, sharpen and median;
tiled neighbor padding, selection clipping, filter preview and Fade Last Filter.
Gaussian radius 20 on 6000×4000 must still meet the source's under-two-second
single-thread target. M4 retains the other seven adjustments and five filters,
plus phase-wide acceptance. Editable smart filters remain outside v1.

The P5 identifiable Photoshop feather comparison is still pending. Physical pen
validation retains its previously approved deferral. Neither `p5-select` nor
`p6-filters` is created. Stop before beginning M3; P7 is outside this phase.
