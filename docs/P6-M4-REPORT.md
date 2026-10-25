# P6 M4 - Remaining adjustments and filters

Work resumed on 2026-10-25 from accepted M3 at `e24b231`, with explicit user
authorization. The complete original P6 section on printed pages 14-15 and all
milestone records were read. Work stays directly on main, without a worktree or
agents. The release marker is the annotated `p6-filters` tag: it is created only
after final-head CI/deployment, artifact inspection and deployed hardware
regression pass. Its annotation records the immutable completion evidence.
P7 has not started.

## Delivered implementation

- Vibrance, color balance, black & white, invert, threshold, posterize and
  gradient map work as destructive adjustments and editable adjustment layers.
  Layer settings, initial selection masks, painting, groups, clipping, opacity,
  fill, blends, history, streaming projects and recovery use the existing paths.
- Color balance has shadow/midtone/highlight RGB offsets and optional luminosity
  preservation. Black & white has six hue weights. Gradient map supports 2-16
  ordered editable position/RGB stops. All controls use deterministic sRGB math
  specified in P6-DESIGN.md; no Photoshop formula equivalence is claimed.
- Motion blur, high pass, uniform/Gaussian seeded monochrome/color noise, emboss
  and pixelate use immutable-input, selection-clipped tile jobs with neighbor
  halos, exact command undo/redo, alpha lock, paging, cancellation and refusal.
  Pixelate cells anchor to the document origin and crop at canvas boundaries.
- GPU viewport previews use the existing temporary source cache and full layer
  compositor. Motion/emboss directions follow document rotation and flip; noise
  uses document coordinates; pixelate uses document-axis cell averages. Final
  Apply runs at document resolution in Rust. Fade retains exact endpoints for
  every filter, with opacity and all 16 existing blend modes.

All twelve adjustments and ten filters now have implementation and controls.
M1-M3 reports document the preceding histogram, command, layer and preview
foundations. Smart filters remain excluded by the user's decision.

## Processing and resource contract

The five processing crates remain independent of browser/UI/GPU types. No
document pixels are processed in TypeScript. Straight-alpha document samples
and hidden RGB whose output alpha remains zero stay exact; spatial temporaries
use premultiplied sRGB. Point adjustments preserve alpha and hidden RGB exactly.
New controls validate before allocation. The existing schema-3 metadata and v2
stream framing require no migration or new dependency/license.

The logical budgets remain 768 MiB CPU and 128 MiB GPU, including document,
history, temporary and GPU buffers. Image files retain the 128 MiB limit and
streamed projects the 4 GiB limit. Filter scratch remains eight MiB, with the
existing budgeted allocator reservation and paged immutable generations.
Preview uses the existing 64-sample halo and 2128-byte filter uniform buffer.
These limits account for buffers, excluding retained allocator pages and
browser/driver overhead. Fitted/rotated nearest-resampled preview remains an
interactive approximation; full-resolution Apply and actual-pixel comparisons
have separate checks.

M3 scheduling is preserved: busy command work pauses ordinary GPU redraws,
prepared preview presentation retains its exception, and each frame RPC waits
for submitted GPU completion. Snapshots, backing size and preview acknowledgements
remain captured at submission across asynchronous waits.
The frame's timestamp also describes submission, while its RPC continues to
wait for completed GPU work. A delayed fence does not consume a later RAF input
slot. No completion fence, busy redraw pause or preview acknowledgement was removed.

## Verification and release gates

All 115 Rust tests, both clippy targets, Rust/JS formatting, TypeScript, ESLint,
16 Vitest cases and the 450-expression license inventory pass. The optimized
release is 3,588,418 bytes / gzip9 1,344,806 bytes, below 1,500,000.
All 144 installed Chrome/Edge M4 feature cases pass on WebGPU/WebGL2, with zero
failures, skips or retries. The frozen Windows shell builds and both hardware
WebView2 APIs pass the expanded smoke, including all seven remaining adjustments
and ten filters. Before the final submission-timestamp correction, the full
deployed 368-case hardware suite and 272-case CI software suite also passed,
without failures, skips or retries. Initial exact-head
[CI 40177491873](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40177491873)
and [deployment 40177491972](https://github.com/MohamedSadat/cashgear-fotoeditor/actions/runs/40177491972)
were inspected, including both software WebView2 backends and CI size
3,589,393 bytes / gzip9 1,344,838 bytes.

The timestamp correction passes all 16 Vitest cases, including the existing
cross-RAF ownership regression with an added no-overlap assertion, and eight
focused hardware preview/cancellation/refusal cases on all four installed
browser/API combinations. Final acceptance repeats the complete 368-case deployed
hardware suite, final-head CI's 272 software cases, both hardware WebView2 APIs,
delivery headers/MIME and size. Each browser case must have exactly one passed
attempt, with no skips or retries. Final run links and inspection results are
recorded in the completion tag and task completion; software is functional
coverage, never hardware performance evidence.

Independent tests cover color vectors, alpha 0/1/128/255, fractional selection,
destructive/layer parity, streamed cold masks/hidden RGB, spatial tile/canvas
boundaries, random hash vectors, repeated seeds, identities, locks, refusal and
exact history. The new browser matrices cover every blend/group/clipping scene
for each color control and all nine odd canvas tiles for each filter variant.
Maximum radii, both noise distributions, both monochrome/color choices,
non-divisor pixelate size 37 and half-pixel-angle regressions are included.

## Corrections found during development

- Half-byte f64/f32 color rounding differences were amplified by Color
  Burn/Dodge and nonseparable blends. The new color transforms stabilize output
  at 1/1024 byte before final quantization. Black & white evaluates equivalent
  linear byte expressions for its six sectors, avoiding hue wrapping/division.
- The first pixelate GPU comparison differed by up to ten channel values at
  exact cell boundaries. Floating division could place `37/37` below one;
  integer cell indexing corrects this. All eight size-37/64 probes pass on
  installed Chrome/Edge and both APIs. Raw failure/trace and corrected evidence
  are `p6-m4-pixelate-first-failure.json`, `p6-m4-pixelate-first-trace.zip` and
  `p6-m4-pixelate-correction-hardware.json` under bench/results.
- CPU motion/emboss offsets now use the same f64 trigonometry as preview
  preparation, including radius-one 60-degree cases.
- A WGSL function name conflicted with the uniform enumerant; renaming it
  restores Naga validation. No shader validation check was removed.
- New browser helpers initially used the wrong preview/save assertions and
  later observed a null first snapshot. They now use acknowledged timing,
  streamed layer-project APIs, canonical reopened IDs, and the established
  first-snapshot/revision wait. The null fixture's failed report and trace are
  preserved as `p6-m4-null-ui-fixture*`. Assertions and deadlines remain intact.
- Screenshot review found gradient-stop controls crowding their remove button;
  each stop now uses two columns with its remove button below the fields.
- Benchmark autosave races originally interrupted histogram validation and
  pre-edit admission. The harness retries only the exact worker refusal that
  occurs before a job starts, on an unchanged revision, with a 60-second bound.
  Admission waits/refusal counts are reported outside successful Apply timing;
  a started or failed job is never retried. Read-only histogram retries are
  also outside Apply timing. Partial failed reports remain in bench/results.
- The noise benchmark now uses the range input's valid 0.01 step multiples;
  browser range sanitization previously disagreed with the expected value.
  Adjustment preview screenshots have distinct names to avoid overwriting the
  filter screenshots.
- Supplemental navigation/brush measurements exposed an M3 timing mismatch:
  the frame RPC reported completion time as its presentation slot. The worker
  now reports the captured submission timestamp while retaining the complete
  GPU wait, immutable frame snapshot and captured preview acknowledgement.
  Earlier raw records retain the `before-submission-time` suffix. The remaining
  completion-backed throughput and latency limits are reported below.

## Measured hardware performance

Windows 10 Pro, Xeon E5-2680 v3 at 2.50 GHz, 31.92 GiB installed RAM and
NVIDIA GTX 1650 (Turing). Installed Chrome 156.0.8078.47 and Edge
156.0.4314.42 each run on verified hardware WebGPU and WebGL2, at
1280×850 / DPR 1. Benchmarks run serially, without another build or GPU suite
in this project. This is a shared workstation: unrelated cashgear-archive CPU
test jobs were observed during the final refresh and were not stopped.
The earlier isolated Gaussian cross-check on the identical WASM passed all
24 calls, with maximum 1.476 s. Final results below use the corrected worker
and retain the observed host-load limitation.

The filter/preview dataset is the resident opaque 6000×4000 NASA/JPL-Caltech/IPGP
photo, cropped without resampling from the official 6002×4000 JPEG:
[Setting the Wind and Thermal Shield](https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/).
Fixture SHA256 is `603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`.
Processing uses one engine worker thread. This is not a disk-cold paging workload.

### Gaussian radius 20 on 24 MP

One first and five warm applications per pair start from the original unselected
raster after exact Undo, presentation and idle refinement. API return covers the
committed command; visible time additionally waits for acknowledged GPU completion.
An independent full-image histogram visits all 24 million samples after every
call. Its work and admission waits are outside Apply timing. With five warm
samples, p95 and p99 both select the slowest warm observation.

| Browser / API   | First Apply ms | Warm p50 ms | Warm p95/p99 ms | Visible p95 ms |
| --------------- | -------------: | ----------: | --------------: | -------------: |
| Chrome / WebGPU |        1327.43 |     1319.23 |         1389.63 |        1633.23 |
| Chrome / WebGL2 |        1382.82 |     1343.17 |         1445.81 |        1666.52 |
| Edge / WebGPU   |        1464.18 |     1370.47 |         1473.89 |        1683.37 |
| Edge / WebGL2   |        1335.72 |     1329.08 |         1551.43 |        1733.38 |

All 24 applications pass the 2,000 ms target; maximum Apply is 1,551.43 ms.
Peak logical CPU residency is 304 MiB and GPU residency 16.110 MiB.
All 24 histogram weights are 6,120,000,000, with 24,000,000 pixels.

### Additional M4 filters

One first plus two warm applications per pair (60 calls total). The table shows
maxima across the four pairs; with two warm samples, p95/p99 are the slower one.
These filters have no separate 2-second requirement. All histogram checks and
768 MiB CPU / 128 MiB GPU assertions pass; peak CPU is 304 MiB.

| Filter and settings                       | Maximum first ms | Worst warm p95/p99 ms |
| ----------------------------------------- | ---------------: | --------------------: |
| Motion blur radius 20, 31°                |          2310.85 |               2787.68 |
| High pass radius 20                       |          1924.23 |               2112.77 |
| Color Gaussian noise amount 0.1, seed 123 |          3884.69 |               3916.21 |
| Emboss radius 3, 135°, depth 1            |           994.11 |               1048.47 |
| Pixelate size 32                          |           692.16 |                684.30 |

### Warm viewport controls

Ten filter controls at fit and actual pixels each contribute 800 DOM input
events. Eight representative adjustment controls, including nested color-balance
and black-and-white arrays and gradient-stop numeric entry, contribute 640 events.
Every event is measured from input through the matching completed React status
and following UI frame. API probes add one first plus ten warm calls for each of
10 filters per view and all 12 adjustments per pair. Actual-pixel API probes use
maximum supported filter radii and pixelate cell size.

Table entries are the worst control p95 / maximum event in milliseconds within
each pair; all 2,240 events pass 100 ms. Raw p50/p95/p99 and individual samples
remain in the JSON records.

| Browser / API   | Filters fit p95 / max | Filters actual p95 / max | Adjustments fit p95 / max |
| --------------- | --------------------: | -----------------------: | ------------------------: |
| Chrome / WebGPU |         48.00 / 64.52 |            79.64 / 80.11 |             47.05 / 47.22 |
| Chrome / WebGL2 |         60.68 / 63.83 |            64.57 / 81.21 |             46.92 / 62.68 |
| Edge / WebGPU   |         62.83 / 64.46 |            81.02 / 81.06 |             43.47 / 47.14 |
| Edge / WebGL2   |         48.18 / 64.52 |            63.07 / 64.51 |             47.11 / 79.76 |

Initial source preparation and first shader/GPU initialization are separate:
filter source preparation is at most 187.16 ms fitted / 86.74 ms actual pixels;
first filter update reaches 1,858.76 ms fitted / 1,809.67 ms actual pixels on
WebGL2. Adjustment preparation reaches 163.06 ms and first update 1,194.17 ms.
These first-use costs do not satisfy a 100 ms cold-start interpretation and are
not included in the warm-slider claim. Maximum warm API call is 49.78 ms for
filters and 38.70 ms for adjustments.

Preview peak logical residency is 105.063 MiB CPU / 26.616 MiB GPU for filters
and 104.063 MiB CPU / 22.489 MiB GPU for adjustments. Logical capacities exclude
retained WASM allocator pages and browser/driver overhead. Fitted and actual-pixel
screenshots were reviewed, including the gradient grid and pixelate controls.

### Supplementary navigation and brush probes

The existing deterministic 6000×4000 fixture supplies three image opens per pair,
four-second pan/zoom/mip-crossing probes and two four-second 300 px soft-brush
probes after a priming dab. Final open times are 0.97–1.29 s. Steady pan/zoom runs
at 59.25–60.00 fps; mip-crossing zoom runs at 57.51–59.75 fps, retaining two-frame
p99 tails. This is not a claim of an uninterrupted 60 fps in every workload.

| Browser / API   | Brush fps range | Worst sample-to-completed-GPU-ack p95 ms |
| --------------- | --------------: | ---------------------------------------: |
| Chrome / WebGPU |     59.94–60.07 |                                    22.18 |
| Chrome / WebGL2 |     49.21–49.61 |                                    34.00 |
| Edge / WebGPU   |     59.64–59.70 |                                    20.72 |
| Edge / WebGL2   |     48.10–50.63 |                                    32.66 |

Brush acknowledgements now include the M3 GPU completion fence; P2 originally
measured through submission. These timings are not directly interchangeable.
The current completed-GPU acknowledgement p95 exceeds one 60 Hz frame, and
WebGL2 brushing does not reach 60 fps. Neither result is reported as passing the
older submission-latency/brush-rate target. The timestamp correction fixes the
extra RAF slot loss (the preceding WebGL2 probe was about 30 fps); it does not
remove the completion-fence cost or assert physical input-to-scanout latency.
The superseded viewport probe retains its 1.51 s first-open observation.

### Evidence and reproduction

Frozen local WASM SHA256:
`97b26ab0c2be3af375024a582ba7d52c55ce4fa4798d2f69b9e8349d1f149e5d`.
Frozen corrected worker SHA256:
`e16dc17bef72dcf30f30fa2f7e8356b83dd0a90e0d09305ead7a746ce7ef69d6`.
The optimized release rebuild reproduced the WASM exactly. Deployed builds have
their own recorded artifact hashes; final delivery is verified separately.

Raw records under `bench/results/`: `p6-m4-filter-performance.json`,
`p6-m4-remaining-filter-performance.json`, `p6-m4-preview-performance.json`,
`p6-m4-preview-actual-performance.json`,
`p6-m4-adjustment-preview-performance.json`,
`p6-m4-viewport-performance.json` and `p6-m4-performance.json` (brush).
Use `pnpm bench:filters` (six trials), `PERF_FILTERS=remaining` plus
`PERF_TRIALS=3` for additional filters, `pnpm bench:filter-preview` at fit then
`PERF_VIEW=actual`, and `pnpm bench:adjustment-preview`. The viewport and brush
scripts accept `PERF_PHASE=p6-m4`. Run them serially against the frozen release.
Failed, before-isolation and before-submission-time records remain available.

## Deferred scope and next phase

All twelve adjustments, ten filters, editable adjustment layers, viewport
previews, histogram and Fade are delivered across M1-M4. Smart filters remain
excluded. The proposed next phase is P7 transforms, crop, resampling and guides.
Stop at this boundary; no P7 implementation is authorized by M4.

P5's identifiable Photoshop feather comparison remains pending without
`p5-select`. Physical Windows Ink pen validation retains its explicitly approved
deferral. Neither synthetic tests nor software rendering substitutes for those
missing reference/physical checks or for measured GPU hardware performance.
