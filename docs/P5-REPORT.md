# P5 selections, fills and clipboard — verification report

P5 implementation is available on main. Acceptance remains open: the required
Photoshop feather comparison has no reference, and no exception has been approved.
Do not publish `p5-select` or begin P6 while this acceptance item is pending.
The report records the implemented scope and its verification before the phase boundary.

## Shipped behavior

Gray8 tiled selections support new/add/subtract/intersect, Shift/Alt modifiers,
rectangle, ellipse, freehand and polygonal lasso, wand and foreground color range.
Controls include tolerance, contiguous matching, all-layer sampling and default-on
anti-aliasing. Select all/none/inverse, feather, grow, shrink, border and smooth
use bounded Rust jobs. GPU presentation provides animated marching ants and the
red Quick Mask overlay. Quick Mask supports brushes, erasers and fills.

Brush, eraser, bucket, Edit Fill and Clear apply selection coverage once and
preserve every channel outside coverage, including hidden RGB. Bucket connectivity
cannot cross zero-selection gaps. Every mutation uses an exact command with
undo/redo. The reusable Rust coverage contract is ready for P6; no P6 filter
implementation is included.

Copy, Cut, Copy merged, Paste and Paste in place use worker-owned exact straight
RGBA samples. Paste always creates a raster layer. The image Clipboard API reads
and writes PNGs; permission denial keeps a visible internal-clipboard fallback.
Own-origin clipboard tokens preserve internal exact samples when supported;
arbitrary external clipboard conversions can normalize transparent RGB. Encoding,
decoding and pixel extraction stay in Rust. Layer masks/Quick Mask must be exited
before copy/cut; fills and painting support those targets.

Active selections are session editing state. They participate in exact history,
unique tile accounting and paging, but are omitted from project/recovery files.
Selection-only revisions do not regenerate thumbnails, recomposite pixels or
rewrite a completed recovery. Existing file formats, shared 768 MiB logical CPU,
128 MiB GPU and 128 MiB image limits are retained. See [design](P5-DESIGN.md) for
admission, cancellation, display mirror and algorithm contracts.

## Wand hardware measurement

Measured serially on 2026-10-24, Windows 10 Pro 10.0.19045, Xeon E5-2680 v3
2.50 GHz, approximately 32 GiB RAM, NVIDIA GTX 1650 / driver 27.21.14.5751.
Chrome 156.0.8078.47 and Edge 155.0.4283.62 reported vendor `nvidia`, architecture
`turing`, non-fallback WebGPU adapters; WebGL2 identified NVIDIA D3D11 ANGLE.
Viewport 1280×850, DPR 1, installed headless browsers, normal document renderer.
No software-renderer result is used as hardware performance.

The photograph is NASA/JPL-Caltech/IPGP PIA22955, [Setting the Wind and Thermal
Shield](https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/).
The official JPEG is actually 6002×4000. The Rust `selection_fixture` example
crops [1,0,6000,4000], without resampling, into a 15,344,787-byte RGB8 PNG.
JPEG SHA-256: `593254364f029a09641769fed56c2c9b8c7bf6371d2042dc1f2edc2c730738da`.
PNG SHA-256: `603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`.
The credited photograph is an ignored local benchmark asset, not a distributed
product asset; see [NASA media guidance](https://www.nasa.gov/nasa-brand-center/images-and-media/).

Each fresh context opens the image before testing. Source samples are resident;
"first" means the first wand invocation in that worker, not disk-cold paging.
One first and five warm trials run per scenario, retaining selection history and
deselecting between trials. Seed [3000,2000], active raster sampling, AA on.
Tolerance 32 selects 2,364,448 binary connected pixels (9.85% of the image), or
2,970,734 global matches (12.38%). Every case matches all source samples; connected
traversal and mask conversion are included. Selected counts precede edge AA.

| Browser / API   | Connected first |     Warm p50 / p95 | Global first |     Warm p50 / p95 |
| --------------- | --------------: | -----------------: | -----------: | -----------------: |
| Chrome / WebGPU |       156.71 ms | 139.45 / 159.77 ms |    101.54 ms | 110.91 / 123.74 ms |
| Chrome / WebGL2 |       133.03 ms | 134.61 / 158.32 ms |    104.96 ms | 112.30 / 126.85 ms |
| Edge / WebGPU   |       149.45 ms | 120.24 / 120.98 ms |     99.82 ms | 116.83 / 124.24 ms |
| Edge / WebGL2   |       148.73 ms | 130.83 / 164.42 ms |    104.13 ms | 113.31 / 135.36 ms |

All 48 tolerance-32 API-return measurements were below 200 ms. Maximum return
164.42 ms; maximum observed visible acknowledgement 188.20 ms. API-return time
includes bridge dispatch, bounded matching/flood/coverage, yields and atomic
command commit. Visible time additionally waits for the normal frame snapshot;
it is not a GPU timestamp or physical scanout measurement. With five warm samples,
the nearest-rank p95 and p99 both equal the slowest warm trial.

Another 24 trials use tolerance 255, selecting all 24,000,000 pixels in
5.69–8.57 ms. That exact sparse shortcut does not scan or flood samples and is
reported separately, not substituted for the tolerance-32 workload.

The raw report includes stage timings, category maxima, sampled simultaneous CPU
peaks, adapter information and the built WASM SHA-256. Maximum sampled CPU was
236.95 MiB, GPU 16.94 MiB. The conservative sum of all CPU category maxima plus
the additional 32 MiB priming allowance is 305.07 MiB, below 768 MiB; these maxima
need not occur together. Up to 32 MiB of admitted
allocator priming can occur synchronously between frame samples; add it
conservatively when interpreting the sampled CPU bound. Logical residency excludes
retained WASM allocator pages, browser objects and driver overhead; it is not
process-memory or VRAM measurement. The authoritative 768/128 MiB limits are
checked independently of this sampling. Evidence:
`bench/results/p5-selection-performance.json`.

## Existing hardware regressions

The unchanged 6000×4000 deterministic PNG and the same browser/API matrix measured
the normal inactive-selection path after the final production build:

| Browser / API   | First / two warm opens | Navigation fps, minimum | Worst period p95 / p99 | Brush fps, two trials | Worst pointer acknowledgement p95 / p99 |
| --------------- | ---------------------: | ----------------------: | ---------------------: | --------------------: | --------------------------------------: |
| Chrome / WebGPU |   1.17 / 1.04 / 0.97 s |                  59.998 |       16.78 / 16.84 ms |         60.06 / 60.06 |                        13.30 / 15.47 ms |
| Chrome / WebGL2 |   1.16 / 1.07 / 1.05 s |                  59.998 |       16.78 / 18.07 ms |         60.16 / 59.73 |                        16.09 / 18.12 ms |
| Edge / WebGPU   |   1.15 / 1.06 / 0.99 s |                  59.998 |       16.74 / 18.68 ms |         59.87 / 59.75 |                        10.52 / 13.17 ms |
| Edge / WebGL2   |   1.26 / 1.17 / 1.05 s |                  59.999 |       16.73 / 16.76 ms |         59.89 / 59.90 |                        14.16 / 16.19 ms |

Navigation uses four-second pan, zoom and cross-mip zoom trials. Frame periods
measure observed worker presentation cadence. At the end of cross-mip zoom,
the remaining refinement acknowledgement took 23.92–45.44 ms; its cached preview
was already visible. The 300-pixel soft brush uses 10% spacing at 100% zoom,
native protocol input at 120 Hz / 1800 pixels/s, one priming dab and two four-second
trials per API. Pointer timing runs from the first sample changing a rendered
pixel to main-thread frame acknowledgement, including queueing, CPU, upload and
submission. It excludes compositor/scanout and physical pen latency. Worst brush
frame-period p99 was 25.16 ms; Chrome/WebGL2's pointer p99 exceeds one 16.67 ms
frame while its p95 remains below it. Do not describe every sample as sub-frame.
Evidence: `p5-viewport-performance.json` and `p5-brush-performance.json`.

## Verification status

- Native formatting/clippy, WASM clippy and all 79 Rust tests pass, including
  500 random selection-history sequences and independent packed-AA comparisons.
- TypeScript, ESLint, Prettier, all 16 Vitest tests and the unchanged 450-expression
  dependency license inventory pass. No new external dependency was introduced.
- Optimized release WASM: 3,229,550 bytes; gzip level 9: 1,231,998 / 1,500,000.
- Windows/WebView2 155.0.4283.62 builds and passes both APIs, including selection
  fill/cut/new-layer paste/exact undo, Quick Mask pixel isolation, existing image
  samples, streamed projects, recovery and all GPU blend references.
- All 144 local installed Chrome/Edge hardware cases pass on WebGPU and WebGL2,
  with zero failures, skips or flaky retries. This includes all 36 P5 cases plus
  the full existing brush/history/layer/recovery/viewport regressions.
  Evidence: `bench/results/p5-hardware-browser-tests.json`.

The broad browser runs exposed test preconditions: the new toolbar moved
layer rows under a sticky inspector header during native drag, and a selection
assertion read before the first frame snapshot. Tests now center the whole layer
stack before drag and await snapshot availability. An overlapping probe also
removed another run's trace directory; final evidence is gathered serially.
The software pass also exposed an export assertion racing queued brush input;
the selection test now waits for that stroke's committed history entry before
reading samples. The lasso pointer-move defect found during integration was fixed and is covered
by an actual rotated/flipped-view lasso, polygon and modifier gesture test.

Remote verification is retained in [FotoEditor checks on main](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/workflows/ci.yml?query=branch%3Amain).
The latest [Azure deployment](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/workflows/azure-static-web-apps-proud-pebble-07b8b4f10.yml?query=branch%3Amain)
includes documentation follow-ups; a newer main push supersedes an unfinished deployment.
The CI result and its `phase-verification` artifact retain software browser and
WebView2 outcomes; software rendering establishes function only. Documentation
follow-ups receive their own checks on main. The public endpoint is
[CashGear FotoEditor](https://proud-pebble-07b8b4f10.2.azurestaticapps.net).

## Pending and deferred work

The Photoshop visual feather comparison is pending. The user has neither a
reference mask nor a local Photoshop installation. The reproducible 513×321
Gray8 input and radii 1/3/8/24 outputs are prepared by `feather_fixture`; their
existence does not establish Photoshop equivalence. The implementation uses
three box passes approximating Gaussian sigma=radius, so matching radius numbers
must be validated against an identifiable Photoshop version/reference.

Named selection channels remain the roadmap's optional follow-up. AI Select
Subject stays outside v1 by user decision. The previously approved physical-pen
validation exception remains unchanged; synthetic input tests and software GPU
checks do not replace physical pen or hardware performance evidence.

Proposed next phase, after P5 acceptance closes: P6 adjustments and filters,
using the existing selection-coverage and command foundations. Stop before P6.

## Reproduction

`pnpm build`, `pnpm exec playwright test`, `pnpm desktop:build` and
`pnpm desktop:smoke` reproduce the functional gates. Use the installed Chrome and
Edge hardware configuration; `GPU_TEST_SOFTWARE=1` is functional coverage only.

Download the official JPEG linked from the NASA page above to
`bench/results/fixtures/p5-photo.jpg`, verify its hash, then run:

```text
cargo run --release -p fotoeditor-io --example selection_fixture
pnpm bench:selection
```

`PERF_PHASE=p5` with `node scripts/viewport-bench.mjs` or
`node scripts/brush-bench.mjs` records the existing regression workloads.
Run hardware benchmarks one at a time after the production build is frozen.
