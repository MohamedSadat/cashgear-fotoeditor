# P9 M1 - Clone Stamp

Implementation, hardware measurements and local verification are complete.
Acceptance requires exact-final-head remote CI/deployment and artifact inspection;
the milestone completion message records those immutable results. This is the
first P9 milestone, not completion of P9; `p9-retouch` remains gated by M2-M5.

Work started on 2026-10-27 directly on main at `af66e10` / `p8-files`, without
worktrees or agents. Read the complete original P9 section on printed page 17
as text and a rendered page, with its supplied SHA-256 verified. P9-DESIGN.md
records the scope and milestone criteria before implementation. The user's
choices persist: clone first, CashGear presets only, mirror/radial symmetry,
editable multi-stop gradients.

## Delivered behavior

- Clone Stamp button / S shortcut, Alt-click source picking and a document-
  coordinate source indicator. Aligned mode retains the initial integer offset
  between strokes; non-aligned mode restarts from the picked point. A new source
  resets alignment; new/open/recovery and worker replacement start without one.
- Current-layer sampling reads raw straight RGBA. All-layer sampling captures an
  immutable pre-stroke visible composite with masks, groups, adjustments, blends,
  opacity and fill. A stroke never reads its own candidate pixels. Foreground
  color is irrelevant; transparent source samples are source-over no-ops.
- Existing procedural tips, pressure/tilt curves, spacing, hardness, flow,
  opacity, stabilizers, pencil and true-size outline apply. Selection coverage
  clips destination writes once. Source/destination respect the visible crop;
  retained outside samples, guides and DPI remain unchanged. Unaffected hidden
  RGB stays exact, and undo restores edited samples byte for byte.
- One named Clone Stamp command per stroke, exact undo/redo, no-op redo
  preservation, alpha lock, pixel/all/ancestor lock refusal. Clone Stamp explicitly
  requires a raster pixel target rather than a layer mask or Quick Mask target.
  Escape/focus loss, queue overflow, missing backing and budget refusal abandon
  the preview without publishing pixels or clearing redo.
- Source generations share immutable handles and are counted uniquely with
  document/history/display/candidates. The worker requests only the next dab's
  sampled source tiles. Hydration reduces remaining stroke admission. Existing
  active-target hydration/admission remains; oversized targets can be refused.

Pixel processing remains in Rust. The five processing crates have no browser/UI/
GPU types. The engine worker owns document/history/GPU/tool-source state; React/
zustand owns tool controls. Projects store document pixels/metadata, without clone
anchors or history. No dependency was added; 459 license expressions remain approved.

## Processing and resource limits

Opaque clone rows use the same UNORM16 coverage and source-over reference as the
general alpha/selection path. Flat one/two-raster Normal preview plans use bounded
Rust CPU composition to avoid float GPU readback and re-upload. Complex plans
retain the GPU route. Small normal/opaque/straight row kernels allow browser WASM
tiering without merging the loop into the general sixteen-mode function.
CPU/GPU and per-sample/row comparisons retain the one-channel tolerance.

The single-frame completed-GPU-work fence remains. Its first sixteen polls yield
through message tasks; subsequent polls use a one-millisecond timer scheduled
from a message task, avoiding nested-timer clamping. Slow queues stay bounded;
cancellation/disposal checks remain. There is no unbounded frame submission.

Budgets remain 768 MiB logical CPU / 128 MiB GPU, 16,384-pixel retained/visible
sides, 128 MiB encoded images and 4 GiB streamed projects. The CPU compositor
admits floating reference and straight output together: at most 2.25 MiB for a
flat tile. Candidate coverage, immutable sources, paging/I/O, history and display
residency participate. Logical capacities exclude map/Arc/allocator retention,
browser/process and driver overhead; these are not physical RAM/VRAM claims.

## Measured hardware performance and approved exception

The user explicitly accepted the complete measured M1 limits and deferred the
all-layer 60 fps target to P12. The original all-layer target fails; it is not a
passed performance criterion. Other acceptance requirements remain unchanged.
See DECISIONS.md for the explicit answer.

Windows 10 Pro 10.0.19045; Xeon E5-2680 v3 at 2.50 GHz; 34,276,601,856 bytes RAM;
NVIDIA GTX 1650/Turing. Installed Chrome 156.0.8078.48 and Edge 156.0.4314.42,
1280x850 / DPR 1, run serially on verified non-fallback NVIDIA WebGPU and NVIDIA
ANGLE/D3D11 WebGL2. No Codex tests/builds run during timing. This is a shared
machine, not a dedicated-host claim. Software CI establishes function only.

The deterministic dense 6000x4000 RGBA8 fixture is 67,038,649 bytes, SHA-256
`809669e7f406d83b59d39fe659411b32c8d014cf6d54e189ba86165ddf82e7f5`.
Both modes show the ordinary document with populated visible tiles and no
diagnostic texture, at 100% zoom. Current mode edits its raster; all mode adds a
separate transparent retouch raster above the image. The source offset is
`[0,-180]`. Size 200, soft round, 100% opacity/flow, 10% spacing, mouse pressure one.
No selection, masks, guides or crop are used in this throughput dataset;
functional tests cover those contracts separately.

One priming dab is timed through commit, followed by two four-second strokes.
Each stroke's first completed dab is acknowledged before steady timing begins;
that start response is reported separately. Native protocol input requests 120 Hz
on an 1800 px/s triangular path with vertical sinusoidal motion. At most four
native input acknowledgements are in flight. Actual requested/delivered counts
are retained; browser raw-event coalescing is not presented as 120 distinct
delivered events each second. OS/file caches are not cleared; these are
first-use/resident/warm measurements, not disk-cold trials.

| Mode    | Browser / API   | Presentation fps range | Changed-pixel acknowledgements/s |
| ------- | --------------- | ---------------------: | -------------------------------: |
| Current | Chrome / WebGPU |            59.98-60.01 |                      46.29-51.79 |
| Current | Chrome / WebGL2 |            59.79-59.98 |                      45.84-51.77 |
| Current | Edge / WebGPU   |            59.98-59.99 |                      46.05-52.02 |
| Current | Edge / WebGL2   |            60.01-60.02 |                      44.57-50.80 |
| All     | Chrome / WebGPU |            28.65-29.87 |                      24.14-25.91 |
| All     | Chrome / WebGL2 |            25.89-27.19 |                      20.67-24.44 |
| All     | Edge / WebGPU   |            31.38-32.61 |                      25.65-29.12 |
| All     | Edge / WebGL2   |            29.18-29.37 |                      23.90-26.19 |

Presentation includes unchanged active previews. Changed-pixel acknowledgements
count advances in the submitted stroke's applied-sample timestamp. Repeated
coverage and brush spacing can produce no new pixel value in a presentation;
the presentation rate is not evidence of sixty distinct pixel updates per
second. No input-to-photon or under-one-frame latency claim is made.

| Mode    | Browser / API   | Cadence p50 / p95 / p99 (ms), range across two trials |
| ------- | --------------- | ----------------------------------------------------- |
| Current | Chrome / WebGPU | 16.57-16.81 / 20.31-20.72 / 21.71-22.28               |
| Current | Chrome / WebGL2 | 16.69-16.85 / 19.93-19.98 / 20.91-21.64               |
| Current | Edge / WebGPU   | 16.55-16.63 / 20.38-20.41 / 22.28-23.36               |
| Current | Edge / WebGL2   | 16.69-16.74 / 19.96-20.00 / 21.71-22.33               |
| All     | Chrome / WebGPU | 32.95-33.69 / 50.38-51.07 / 53.56-57.38               |
| All     | Chrome / WebGL2 | 34.16-34.50 / 51.27-52.60 / 63.08-67.02               |
| All     | Edge / WebGPU   | 32.18-32.59 / 38.65-44.92 / 51.42-55.46               |
| All     | Edge / WebGL2   | 33.37-33.42 / 49.18-49.48 / 50.61-65.55               |

| Mode    | Browser / API   | Source-to-completed-GPU-ack p50 / p95 / p99 (ms) |
| ------- | --------------- | ------------------------------------------------ |
| Current | Chrome / WebGPU | 17.64-18.26 / 26.15-26.17 / 26.81-27.38          |
| Current | Chrome / WebGL2 | 21.99-22.69 / 28.33-29.68 / 30.43-31.76          |
| Current | Edge / WebGPU   | 16.29-16.98 / 25.28-25.42 / 26.09-26.41          |
| Current | Edge / WebGL2   | 22.53-22.82 / 29.81-30.24 / 32.80-33.16          |
| All     | Chrome / WebGPU | 35.72-40.74 / 53.94-60.06 / 63.12-69.55          |
| All     | Chrome / WebGL2 | 41.56-44.25 / 61.27-69.00 / 77.62-89.27          |
| All     | Edge / WebGPU   | 33.92-37.16 / 46.32-51.05 / 52.64-63.46          |
| All     | Edge / WebGL2   | 39.98-40.31 / 55.92-57.16 / 65.32-73.90          |

Latency starts at the native sample first changing a submitted pixel and ends
when the main thread observes the worker reply after its GPU completion fence.
It includes queuing, CPU processing, uploads, submission, GPU completion and
message observation. It excludes compositor/scanout and physical pen latency.
Percentiles are empirical per-event/per-frame values within each four-second
trial; two trials do not establish long-term tail confidence.

| Browser / API   | First dab commit, current / all (ms) | New-stroke first-dab ack, current / all (ms) |
| --------------- | -----------------------------------: | -------------------------------------------- |
| Chrome / WebGPU |                       50.11 / 150.82 | 40.42-42.60 / 53.39-55.56                    |
| Chrome / WebGL2 |                       66.62 / 167.03 | 38.48-42.50 / 70.24-75.03                    |
| Edge / WebGPU   |                       50.16 / 167.51 | 42.66-45.75 / 60.34-71.71                    |
| Edge / WebGL2   |                       63.98 / 167.55 | 46.03-54.77 / 58.54-61.20                    |

Observed warm logical CPU peaks are 103.92 MiB (current) and 202.17 MiB (all),
with GPU 32.11 / 20.38 MiB. The all-mode synchronous CPU reference/output
allowance adds at most 2.25 MiB beyond an observed snapshot; it is admitted before
allocation even though snapshots cannot sample a synchronous kernel's middle.
These bounds remain far below 768 / 128 MiB. First-dab commit includes automation,
history/persistence and observation; it is not an isolated kernel measurement.

Raw final matrices, asset SHA-256s and screenshots are
`bench/results/p9-m1-final-{current,all}-performance.json` and matching images/logs.
Reproduce with `pnpm bench:clone`; set `CLONE_ALL=1` for the second mode.
Measured runtime code is `f432fd3`; the local WASM SHA-256 is
`9c99d9422fba9aba15a0c5ebc0f29715f34e67c97d4e746a6df120b7fecdcb9b`.
Later control-layout/report changes do not alter the WASM/worker processing code.

## Verification and earlier investigations

Local static/unit checks pass: Rust formatting, native/WASM clippy, 185 Cargo
tests, TypeScript, ESLint, 16 Vitest tests, JavaScript formatting and 459 license
expressions. Release WASM is 4,460,038 bytes / gzip9 1,647,803 against the approved
2,000,000-byte gate. No limit was raised.

Rust cases cover immutable overlap, source/destination tile/crop edges, alpha,
selection/opacity, alpha/pixel locks, no-op/refused/cancelled edits, exact undo/redo,
cold source requests, row/reference parity and reference/output admission.
Focused browser cases cover real source picking/alignment, blank-current versus
masked/grouped all-layer samples, visible preview cancellation and redo, lock/mask
refusal, lifecycle recovery and all sixteen CPU/GPU blend modes on both APIs.
The final 580-case installed Chrome/Edge hardware regression passes in 27.4
minutes, one attempt each, with zero skips, retries, flaky cases or global errors.
After the final CSS-only control correction, all sixteen focused Clone Stamp
cases pass again in 53.9 seconds with the same clean result. The compiled controls
also fit 900 px and 1280 px viewports without horizontal overflow. The full suite
and this repeat exercise identical WASM/worker processing assets.

Rebuilt final WebView2 passes both NVIDIA APIs, Clone Stamp pixel/command/exact-
undo checks, six extended imports, thirteen PSDs, four export modes and the
existing recovery/geometry smoke. Evidence includes `p9-m1-final-browser-tests`,
`p9-m1-controls-browser`, `p9-m1-controls-desktop-smoke` and control screenshots
under `bench/results/`.

Exact-final-head remote CI/deployment/artifact inspection remains required before
acceptance. The milestone completion message records its final results.
No software adapter result is used as hardware performance evidence.

The first lock fixture used Playwright's synchronous checkbox-state check for
an asynchronous worker command. Screenshot/trace showed the command updating;
the fixture now clicks and awaits the reported lock state. Sixteen focused cases
then pass on both installed browsers/APIs. Initial reports/traces are retained.

Implementation CI `40570505462` is green but has 429 first-attempt passes and
two retried WebGL2 clone fixtures, so it does not certify acceptance. Trace
snapshots show both clicks while the UI still reports "Updating layers...": the
canvas correctly ignores input during an operation. The fixtures now await the
prepared revision, selected mask target and idle presentation before real input,
without retrying clicks or weakening pixel/history assertions. The final-head
run must still have zero retries/flaky cases. Its artifact inspection and the
milestone completion message distinguish this earlier run from certification.
All 36 local software cases (three repetitions of the complete clone matrix)
and sixteen installed-browser hardware cases pass with the corrected fixture,
one attempt each, without skips, retries, flaky cases or global errors. Only
test synchronization and this report changed; runtime/performance code is intact.
The remaining setup/selection preparation paths use the same revision/idle
barrier; the complete twelve-case software clone matrix also passes after that
extension, without retries, skips, flaky cases or global errors.

The first hosted run with the extended setup guard has 38 passes and two Edge
failures: "Engine ready" can precede the first published frame snapshot. The
fixture now explicitly awaits the snapshot attribute before polling its prepared
revision. Local twelve-case software and sixteen-case hardware matrices pass
with that wait, followed by all forty hosted Chrome/Edge clone/GPU cases on one
attempt each, without skips, retries, flaky cases or global errors.
Traces/screenshots are retained in
`bench/results/p9-m1-hosted-first-snapshot-failure`; none of these failed or
superseded runs is acceptance evidence.

Earlier serial-native input delivered only 170-197 requested moves per four
seconds. Later probes corrected bounded input delivery, pressed-button state and
first-frame preparation accounting; zero-change runs are rejected. GPU readback
and monolithic CPU probes are diagnostic history, not the final accepted matrix.
Opaque stamping, row composition/quantization and bounded fence polling preserve
the references. All superseded logs remain under `bench/results/p9-m1-*`.

## Deferred work and next milestone

All-layer clone speed is explicitly deferred to P12. Complex layer stacks retain
the existing compositor route and have no new general 60 fps claim. Physical
Windows Ink retains its approved deferral. P5's identifiable Photoshop feather
comparison stays pending without `p5-select`; P6/P7 limits and P8's accepted
save-speed exception retain their status.

Proposed next milestone: P9 M2, Poisson healing brush and nearby-search spot
healing, including identifiable portrait seam verification. Then M3 PatchMatch
fill, M4 other retouch brushes/gradients, M5 image tips/presets/symmetry and final
phase acceptance. The proposed next phase after P9 is P10 Windows packaging.
Stop at M1; none of those later milestones or P10 starts here.
