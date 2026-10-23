# P4 layers — phase report

Final remote CI and deployment verification are in progress. The completion tag
will be created only after those gates pass. P5 has not started.

## Shipped

The editor now supports raster layers and nested groups, add/delete/duplicate/
rename, drag reorder and Ctrl/Shift selection. New groups pass through their
backdrop; isolated Normal groups are available. Layers expose visibility,
opacity, fill, sixteen sRGB blend modes and pixel/alpha/position/all locks.

Masks can be added, painted, disabled, applied or deleted. Alt-click displays a
mask; the panel identifies the active mask target. Clipping uses the layer below.
Merge down, merge visible and flatten produce raster candidates. Applying an
isolated group's mask rasterizes the group; pass-through groups must first be
changed to Normal because their appearance depends on their backdrop. Flatten
preserves transparency. Every document edit uses a command with exact COW undo
and redo, preserving straight-alpha samples and hidden RGB.

Rust owns pixel processing, thumbnails, the independent CPU reference and the
portable WGSL compositor. GPU composition uses float32 premultiplied temporary
targets and bounded tile readback. The worker retains the source tree separately
from its displayed composite cache. Painting updates affected composite tiles;
panning uses cached composite tiles and mips. Thumbnails refresh after a stroke,
including the active layer's ancestors, and yield to editing.

Immutable generation handles allow cold RGBA8 and Gray8 samples to be paged from
owned OPFS files. Flush and exact readback validation precede eviction. Document,
history and pending jobs retain their backing references. Web Locks protect live
sessions and collection. Unique resident buffers and scratch remain subject to
the existing 768 MiB logical CPU limit; GPU objects remain below 128 MiB.

V2 projects stream layers, groups, masks and exact samples through bounded
records, with full validation before replacing a document. Large saves use a
writable file handle and the approved 4 GiB total-file cap. V1 single-raster
projects remain readable; images retain the 128 MiB limit. Recovery retains three
validated, self-contained generations and preserves completed saves when a
candidate is interrupted, corrupt or cannot fit storage. History/view state are
excluded, as selected in P3. See [implementation notes](P4-IMPLEMENTATION.md).

## Dense hardware workload

Measured on Windows 10 Pro 10.0.19045, Xeon E5-2680 v3 at 2.50 GHz, approximately
32 GiB RAM and NVIDIA GTX 1650, driver 27.21.14.5751. Chrome 156.0.8078.47 and Edge
155.0.4283.62 used NVIDIA/Turing WebGPU and ANGLE D3D11 WebGL2. No software adapter
is included in these performance results.

The deterministic fixture contains **50 distinct dense 4000×3000 RGBA8 layers**,
all visible, with varied alpha and hidden RGB. It has 9,600 distinct tile payloads
and 50 distinct layer hashes. Authoritative padded samples occupy 2,400 MiB. The
stream is 2,517,098,823 bytes, generated with seed `0x9e3779b9`; SHA-256:

`436f470f7765e5dc594b207ef22313e9d43779360a30521ab2e94ad3ca83c0ab`

The native streaming validator read the complete file with zero resident sample
buffers. Browser runs used fresh persistent profiles, the normal document
renderer, 1280×850 viewport, DPR 1 and fitted 16.4667% zoom. Source layers were
actually read and composited before measurement. All 50 thumbnails and initial
recovery work completed before three four-second warm pan trials. OS file caches
were not flushed; cold here means a fresh editor/profile and empty engine caches.

| Browser / API   | Cold copy + validation + composition | Setup through thumbnails and first recovery | First recovery write + validation | Warm fps, three trials   | Worst trial p95 / p99 period |
| --------------- | -----------------------------------: | ------------------------------------------: | --------------------------------: | ------------------------ | ---------------------------: |
| Chrome / WebGPU |                              96.89 s |                                    226.07 s |                           90.35 s | 59.991 / 59.992 / 59.992 |             16.73 / 33.36 ms |
| Chrome / WebGL2 |                             113.63 s |                                    283.88 s |                          136.24 s | 59.741 / 59.993 / 59.733 |             33.34 / 33.40 ms |
| Edge / WebGPU   |                             192.15 s |                                    330.41 s |                           99.40 s | 59.992 / 59.968 / 59.994 |             33.32 / 33.36 ms |
| Edge / WebGL2   |                              89.47 s |                                    202.73 s |                           76.79 s | 59.743 / 59.742 / 59.994 |             33.32 / 33.39 ms |

Median periods were about 16.67 ms in all trials. Rates use acknowledged
presentations over elapsed time; period percentiles use observation timestamps.
They are not GPU timestamp queries or physical scanout measurements. The warm
60 Hz target is met at this fitted view, with occasional two-frame timing tails.
This does not establish full-resolution painting performance on 50 layers or
claim that all source layers are recomposited every frame.

All four runs sampled the same category maxima: document 286.25 MiB, history
0 MiB, temporary 82 MiB and GPU 20.34 MiB. Adding the CPU category maxima gives a
conservative **368.25 MiB upper bound**, rather than a simultaneous measured peak.
Logical capacities exclude map/Arc metadata, retained WASM allocator pages,
browser/driver overhead and total process memory. The source stream stays on
disk; a resident composite cache is counted separately within document memory.

Storage estimates grew from 186 bytes to 5,034,199,330 bytes for the working
project and one completed recovery generation. Reported quota began near 10 GiB
and later increased dynamically. Persistent-storage permission was denied in
these headless runs; writes still succeeded. Recovery generations are
self-contained and each additional dense snapshot needs approximately another
2.52 GB. Private profiles tested during development lacked sufficient quota;
the editor reports that failure and retains earlier valid data.

Cold opening and recovery are substantial costs. No cold-load time limit was
approved for this paged workload. The table reports those costs separately from
warm navigation. Raw evidence is retained locally in
`bench/results/p4-layers-performance.json` and `p4-layer-dataset.json`.
Reproduction is documented in P4-IMPLEMENTATION.md and
`scripts/layers-bench.mjs`.

## Verification

- Formatting; native and WASM clippy; 61 Rust tests; TypeScript; ESLint; 14 Vitest
  tests; production build; all 450 dependency license expressions passed.
- Optimized local WASM: 3,090,986 bytes, gzip level 9: **1,177,969 / 1,500,000**.
- All **108 installed Chrome/Edge hardware browser cases** passed, with zero
  retries, skips or failures. This includes prior-phase regressions, layer-panel
  edits, mask painting/view/export, exact undo, streamed save/reopen and recovery
  after quota failure and a renderer kill during a candidate write.
- GPU readbacks cover all sixteen modes in 64 scenes per API/browser, including
  varying alpha, masks, clipping, pass-through and isolated groups. Every channel
  agrees with the independent CPU reference within ±1. Core tests also check
  expected outputs, exact samples, paging/COW and random command undo sequences.
- The release Windows shell built and launched in WebView2 155.0.4283.62 on both
  hardware APIs. Painting, exact undo/redo, legacy project roundtrip, autosave
  restore, all 64 blend readbacks and byte-identical small layered streamed
  roundtrip passed. WebGPU additionally opened, autosaved, streamed a
  2,517,098,823-byte manual file, reopened all 50 layers / 9,600 tiles and matched
  the exported PNG SHA-256 before/after:
  `f79236f16067d6d63d813ed5ce0b8a4cb030343c563efd6831280e2f15c6965a`.
  Its first dense opening took 88.93 s and streamed manual save 92.92 s. These are
  functional-run timings, not a desktop frame-rate benchmark. The writable file
  test uses an OPFS FileSystemWritable handle; it does not automate a native
  picker dialog. Evidence: `bench/results/p4-layer-desktop-smoke.json`.

Final remote CI and public deployment evidence will be added after their checks
complete. Hardware browser evidence is retained in
`bench/results/p4-layer-hardware-browser-tests.json`.

The earlier foundation checkpoint and its single-layer measurements remain in
PROGRESS.md. They do not substitute for the dense-layer results above.

## Deferred work and limits

No P4 feature is intentionally deferred. The previous explicit P2 exception for
physical Windows Ink validation remains: no pen hardware was available, and
synthetic pressure/tilt coverage does not establish physical pen behavior.

Projects can hit memory, manifest/depth or disk limits before the 4 GiB file cap.
The cap is an upper bound, not a promise that every possible project shape fits.
Large dense composites that cannot fit their reserved cache are refused before
document replacement. Storage eviction/site-data clearing and OS power-loss
durability remain outside the recovery guarantee. PNG export and image input
retain their existing limits. PSD interoperability and color profiles remain P8.

The proposed next phase is **P5: selections, fills and clipboard**. Read the phase
records and resolve its product questions before starting it. Stop here after
P4 completion.
