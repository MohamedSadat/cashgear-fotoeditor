# P6 M4 - Remaining adjustments and filters

Work resumed on 2026-10-25 from accepted M3 at `e24b231`, with explicit user
authorization. The complete original P6 section on printed pages 14-15 and all
milestone records were read. Work stays directly on main, without a worktree or
agents. Phase-wide verification is in progress; this report does not yet
authorize `p6-filters`. P7 has not started.

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

## Verification in progress

All 115 Rust tests, both clippy targets, Rust/JS formatting, TypeScript, ESLint,
16 Vitest cases and the 450-expression license inventory pass. The optimized
release is 3,588,418 bytes / gzip9 1,344,806 bytes, below 1,500,000.
All 144 installed Chrome/Edge M4 feature cases pass on WebGPU/WebGL2, with zero
failures, skips or retries. The frozen Windows shell builds and both hardware
WebView2 APIs pass the expanded smoke, including all seven remaining adjustments
and ten filters. Phase regression, software suite, serial hardware measurements
and final remote gates remain to be completed.

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

## Deferred scope and next phase

Complete the remaining P6 verification before publishing its completion tag.
After P6 acceptance, propose P7 transforms, crop, resampling and guides, then
stop for that phase boundary. No P7 implementation is authorized by M4.

P5's identifiable Photoshop feather comparison remains pending without
`p5-select`. Physical Windows Ink pen validation retains its explicitly approved
deferral. Neither synthetic tests nor software rendering substitutes for those
missing reference/physical checks or for measured GPU hardware performance.
