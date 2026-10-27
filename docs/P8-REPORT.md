# P8 - Files, color and raster PSD interoperability

Implementation and local verification completed on 2026-10-27. The release marker
is the annotated `p8-files` tag, created only after the final commit's remote
CI/deployment and artifact inspection pass. Its annotation records the immutable
head and completion evidence. Work stops before P9.

P8 started directly on main after `86043ed` / `p7-transform`, without agents or
worktrees. The original roadmap's complete printed pages 16-17 were read as text
and rendered pages. User choices are recorded in DECISIONS.md: PSD import only,
BMP/GIF first-frame/8-bit TIFF, sRGB ICC embedding by default, ZIP/LZ4 projects and
the 2,000,000-byte gzip WASM gate. After inspecting real measurements, the user
explicitly accepted P8's measured save limit and deferred the original
under-three-second target to P12. That target fails; it is not reported as passed.

## Delivered behavior

- `.cgfoto` saves use bounded ZIP32 entries: versioned manifest, LZ4 tile blobs,
  thumbnail and visible-canvas composite PNG. Legacy v1 and raw streamed schemas
  2/3/4 remain readable. Exact authoritative layer/mask samples, hidden RGB,
  groups/properties/adjustments, retained outside-crop data, DPI and guides survive
  reopen; history starts fresh and selections remain session state. Preview PNGs
  never substitute for authoritative tiles. Recovery retains its validated raw
  generations and prior-good-snapshot behavior.
- Chromium filesystem pickers open and save handles; Save project reuses the
  current project handle and Save project as chooses another. Wrong extensions
  are rejected before writing. Ten recent handles persist in IndexedDB with
  permission/missing-file errors and forgetting. Inputs/drop and file-backed
  downloads remain fallbacks. No whole 4 GiB project Blob is allocated.
- PNG/JPEG/WebP imports gain BMP, first-frame GIF and classic 8-bit TIFF.
  Supported embedded RGB/gray matrix/TRC ICC profiles convert to sRGB through
  qcms; invalid/unsupported tags are refused explicitly. Tagged BMP V5 metadata
  uses a bounded adapter because the base decoder skips those color fields.
  Linked/calibrated BMP definitions are refused. LUT/CMYK profiles, high-depth
  inputs and BigTIFF remain unsupported. TIFF associated alpha is converted to
  straight samples before ICC conversion; unsupported extra sample definitions
  are refused. A bounded adapter preserves valid profiles on tiny TIFF images.
- Export image provides actual encoded size previews, PNG, JPEG quality/matte,
  WebP quality/lossless and optional sRGB ICC embedding (on by default). Sources
  are the ordinary composite, isolated active raster with mask/opacity/fill, or
  canvas-sized selection coverage. All use the visible crop. JPEG is opaque;
  WebP alpha is exact and lossless formats preserve their chosen RGBA samples,
  including hidden RGB. Groups/adjustments are refused as isolated raster sources.
- PSD v1 8-bit RGB raster import handles raw/RLE/ZIP/ZIP prediction channels,
  layer order/names, supported blends, opacity/fill, visibility, clipping, locks,
  groups and editable masks. Negative/outside bounds expand retained extent while
  preserving the original visible canvas. Unsupported rendering features are
  refused. P8-PSD.md records the precise subset; no general Photoshop or PSB/
  16/32-bit/type/vector/smart-object compatibility is claimed. PSD export and HEIC
  are outside the user's selected scope.

Pixel processing remains in Rust. The engine worker owns document/history/GPU
state. PNG streams scanlines; JPEG/WebP capture immutable visible samples and use
a disposable CPU-only codec worker with an admitted allocator ceiling. The UI
only creates/terminates the sibling worker and transfers a MessagePort. This
avoids the observed packaged WebView2 nested-worker loading failure without
relaxing CSP. Encoded chunks go directly to OPFS and return a file-backed File.
Cancellation waits for file-job cleanup before releasing reservations. Failed
imports/exports/writes preserve document, redo and existing destination files.

Budgets remain 768 MiB logical CPU / 128 MiB GPU, 16,384-pixel sides, 128 MiB
images and 4 GiB streamed projects. Codec, capture, conversion, previews, bridge,
compression and I/O buffers participate. Large imports/exports may be refused
before their maximum dimensions if admission cannot fit. Lossy WebP's VP8 side
limit is 16,383; PNG/lossless WebP allow 16,384. Logical capacities exclude
allocator arenas, browser/process and driver overhead. WebP exports omit DPI;
PNG/JPEG retain supported resolution metadata.

## Independent correctness evidence

Thirteen unmodified MIT public PSDs have pinned upstream revisions, SHA-256s and
preserved notices. Twelve carry identifiable Photoshop VersionInfo; one lacks
VersionInfo and is not attributed to Photoshop. Independent psd-tools decodes layer/mask channels
and Pillow/LittleCMS converts tagged RGB: twelve fixtures match all layer RGBA
exactly, one differs by one red value; all alpha/mask samples are exact. Actual
stored Photoshop white-matte merged RGB and alpha differ by at most one each,
within the two-RGB/one-alpha test limits. No self-roundtrip or library recomposite
generates the Photoshop merged expectations. These masks do not validate P5
feathering or every Photoshop blend/feature.

Independent Pillow/libjpeg/libwebp decodes six tagged/untagged export fixtures:
lossless RGBA, WebP alpha, ICC presence/absence, opaque JPEG and 300 DPI agree.
Native tests also cover quality-dependent lengths, mattes, crop/tile edges,
selection/hidden RGB, masks/opacity, refusal/admission and the WebP side limit.
Project/PSD adversarial cases cover truncations, mutations, directory/reference/
CRC/extent/schema validation and bounded decompression without candidate commit.

Independent Pillow/LittleCMS comparisons of tagged PNG, BMP and associated-alpha
TIFF have maximum RGBA error `[1, 1, 0, 0]`; alpha is exact. Native TIFF regressions
cover conversion order, unassociated samples, zero-alpha RGB and invalid extra
samples. BMP regressions cover profile extents and unsupported color definitions.

The real 20-layer dense save contains 7,680 distinct padded RGBA tiles. Independent
Python zipfile/python-lz4 checks CRCs, decodes every tile and hashes normalized
document metadata with sample hashes substituted for generation references.
Legacy omitted crop/guide defaults normalize to their documented values; file
schema is not document data. Source and saved canonical SHA-256 match:
`7e7073f7c09310418c777f351369f4df961d22429a01ccafe9b8165e65990db6`.
All retained/padded sample bytes participate, including hidden RGB.

## Hardware measurements

Windows 10 Pro; Xeon E5-2680 v3 at 2.50 GHz; 34,276,601,856 bytes reported RAM;
NVIDIA GTX 1650/Turing with non-fallback WebGPU and NVIDIA ANGLE/D3D11 WebGL2.
D: identifies as WDC WD1000DHTZ-60N21V0 through RAID. Installed Chrome
156.0.8078.48 and Edge 156.0.4314.42 run sequentially at 1280x850 / DPR 1 with
fresh D: profiles. Tests/builds do not run
alongside measurements. Software CI proves function, not hardware performance.

The save fixture generator uses seed `0x9e3779b9`, twenty distinct dense 6000x4000
RGBA layers and 7,680 tiles. Raw input is 2,013,676,863 bytes, SHA-256
`2133a854f4ff2b605a44ebe28652d872d5188cdfc47ae43822cd20289917a542`.
The source is opened into immutable raw OPFS backing and fully composited before
timing. Save includes thumbnail/composite, LZ4/ZIP, writes/flush and close to an
actual writable filesystem handle. Hashing/download verification is outside the
timer. OS caches are not cleared; first/warm does not mean disk-cold. Warm
nearest-rank p95/p99 use three samples and are maxima, not reliable tail estimates.

Final measurements use the normal document view, with populated visible tiles
and completed refinement required before timing. An earlier diagnostic-scene
harness is retained as superseded evidence. These production assets were built
from `bee33f95556a581237b40f0f7106bea499bc3fd1`; during measurement the untracked
report draft was the only working-tree change. Measured WASM SHA-256 is
`4af129810f2d0bc600519bf17ed30e18fc8836a28ea02b4818269371618a2583`.

| Browser / API   | First save (s) | Warm p50 / p95 / p99 (s) | Peak logical CPU / GPU (MiB) |
| --------------- | -------------: | -----------------------: | ---------------------------: |
| Chrome / WebGPU |         43.756 | 43.966 / 44.985 / 44.985 |                 605.8 / 20.5 |
| Chrome / WebGL2 |         44.423 | 44.167 / 46.344 / 46.344 |                 609.0 / 20.5 |
| Edge / WebGPU   |         46.090 | 57.957 / 59.804 / 59.804 |                 601.0 / 20.5 |
| Edge / WebGL2   |         55.462 | 59.984 / 60.282 / 60.282 |                 581.5 / 20.5 |

All 16 saves complete in 43.276-60.282 s within residency/file limits. Bounded
4 MiB batching reduces each save from 7,782 packets to 537 actual writes.
The first normal Chrome/WebGPU trial records 20.504 s paging, 12.228 s encoding,
7.569 s writes and 3.150 s cooperative yields. Every trial misses three seconds;
P12 owns that explicitly deferred target. Earlier 65-66 s and buffered 51.952 s
probes remain historical evidence, not substitutes for this complete matrix.

The final archive from each browser/API pair is downloaded and hashed outside
the timer. All four retained archives match the independently decoded variants:
WebGPU is 2,123,367,314 bytes, SHA-256
`9978218f56d7d25ba744bb603f118e9401eb8943b927a15c9b4643c75d9bb1c4`;
WebGL2 is 2,123,367,315 bytes, SHA-256
`88fc34b2d37a202e1a709671c50732f4f01b87aac76e7fd15e3f8d3d8b101d3b`.
Both match the canonical document hash above. Their sole ZIP-entry difference
is the non-authoritative composite PNG: maximum RGB difference one, exact alpha.
Manifest, thumbnail and authoritative tile payloads agree.

The export fixture is the unchanged 6000x4000 photo, 15,344,787 bytes, SHA-256
`603695186d71327778938834ce0418942e8143a9873be7f1e7830f5a13ed91e0`.
All 64 exports complete. PNG runs first, then JPEG quality 85, lossy WebP quality
85 and lossless WebP, each with one first and three warm trials. ICC embedding is
enabled. Total API time includes capture/hydration/persistence, codec startup,
encoding, output flush and File publication. No export latency threshold was
specified. Warm percentiles have the same three-sample limitation as saves.

| Browser / API   | Format        | First (s) | Warm p50 / p95 / p99 (s) |
| --------------- | ------------- | --------: | -----------------------: |
| Chrome / WebGPU | PNG           |     1.314 |    1.341 / 1.502 / 1.502 |
| Chrome / WebGPU | JPEG          |     3.692 |    2.261 / 2.261 / 2.261 |
| Chrome / WebGPU | WebP lossy    |    12.652 | 11.938 / 11.951 / 11.951 |
| Chrome / WebGPU | WebP lossless |     1.862 |    1.717 / 1.833 / 1.833 |
| Chrome / WebGL2 | PNG           |     1.194 |    1.173 / 1.183 / 1.183 |
| Chrome / WebGL2 | JPEG          |     3.712 |    2.248 / 2.300 / 2.300 |
| Chrome / WebGL2 | WebP lossy    |    12.165 | 11.795 / 11.894 / 11.894 |
| Chrome / WebGL2 | WebP lossless |     1.792 |    1.740 / 1.879 / 1.879 |
| Edge / WebGPU   | PNG           |     1.307 |    1.170 / 1.177 / 1.177 |
| Edge / WebGPU   | JPEG          |     3.619 |    2.373 / 2.454 / 2.454 |
| Edge / WebGPU   | WebP lossy    |    12.298 | 11.428 / 11.558 / 11.558 |
| Edge / WebGPU   | WebP lossless |     1.975 |    1.819 / 1.831 / 1.831 |
| Edge / WebGL2   | PNG           |     1.254 |    1.112 / 1.123 / 1.123 |
| Edge / WebGL2   | JPEG          |     3.606 |    2.328 / 2.333 / 2.333 |
| Edge / WebGL2   | WebP lossy    |    12.339 | 11.922 / 12.044 / 12.044 |
| Edge / WebGL2   | WebP lossless |     2.007 |    1.803 / 1.859 / 1.859 |

Encoded sizes agree across all browser/API pairs: PNG 17,980,213 bytes, JPEG
2,069,916, lossy WebP 770,440 and lossless WebP 11,119,126. Peak logical GPU
residency is 16.11 MiB. PNG's peak logical CPU reservation is 356.02 MiB.
JPEG/WebP admit the complete remaining helper allowance, so their sampled
reserved CPU reaches 768 MiB; this is not a measurement of physical memory use.
Actual Rust codec allocation peaks are 171,362,505 bytes for JPEG, 447,129,858 for
lossy WebP and 217,183,268 for lossless WebP. Owner-plus-codec logical residency
at those peaks is respectively 275,751,113, 551,518,466 and 321,571,876 bytes.
Allocator arenas and browser/driver/process overhead are excluded.

Raw methods/runtime hashes/trials are in `bench/results/p8-save-performance.json`
and `p8-export-performance.json`; `p8-export-dialog.png` shows the normal photo
behind the dialog. Reproducible hardware and independent reference runners are
committed under scripts/. All final runtime asset hashes agree between matrices.

## Verification

Local checks: 175 Rust tests, native/WASM clippy, Rust/JS formatting,
TypeScript, ESLint, 16 Vitest cases, 459 license expressions; release WASM
4,444,995 bytes / gzip9 1,640,646 against the approved 2,000,000-byte gate.
The full final hardware regression passes all 564 Chrome/Edge WebGPU/WebGL2 cases
in 26.8 minutes: one attempt each, no skips, retries, failures or global errors.
Expanded rebuilt WebView2 passes both hardware APIs, 13 PSDs and four
export modes per backend. The preview-grid test now uses the actual view center
after the recent-files row changed canvas height; its unchanged pixel assertions
pass all 20 hardware and 15 software preview cases. Prior CI failures/retries
remain recorded rather than being retrospectively reported as passes.

The TIFF correction passes all 44 focused browser cases, six extended inputs in
both hardware WebView2 backends and the independent color checks. File-access
tests use real structured-cloned OPFS handles and controlled permission replies.
The deployed final implementation passes all 44 focused format/export cases on
installed Chrome/Edge and both hardware APIs, one attempt each with no errors,
skips or retries. Its HTTPS page retains COOP same-origin and COEP require-corp.
Implementation CI `40520015674` and deployment `40520015528` pass at `bee33f9`.
Its downloaded artifact confirms all 419 software browser cases pass once, with
no skips, retries, flaky cases or global errors, plus both software WebView2 APIs
with six extended imports, thirteen PSDs and four export modes per backend.
The normal-document save/export matrices pass all residency/file caps; four
retained archives match the independently audited hashes. Exact-final-head
remote CI/deployment and artifact inspection are required before tagging; the
completion tag annotation records those immutable results.

## Deferred work and next phase

The user accepted the measured P8 save limit; the original under-three-second
target is deferred to P12. PSD export and HEIC were not selected. High-depth
processing remains P11; unsupported color/PSD features retain explicit refusals.
P5's identifiable Photoshop feather comparison remains pending without
`p5-select`; physical Windows Ink retains its approved deferral. P6/P7 reported
performance limits retain their status.

Proposed next phase: P9 retouch and advanced painting, beginning with a scoped,
independently accepted first milestone after its product choices are resolved.
Do not start P9 during P8 completion.
