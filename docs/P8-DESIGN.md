# P8 design and validation contracts

Started 2026-10-27 on clean main at `86043ed6e94cbf616dcb68c43e0ebb570029ae5f`.
The user explicitly authorized P8 in a fresh session without worktrees or agents.
AGENTS.md, PLAN.md, PROGRESS.md and DECISIONS.md were read before work. The
complete original P8 requirements and acceptance were read on printed pages
16-17, both extracted text and rendered images. The unchanged source is
`C:/Users/msada/OneDrive/Desktop/Raster Editor — Implementation Plan.pdf`, SHA-256
`83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.
Rendered review pages are ignored evidence under `bench/results/p8-roadmap/`.

This document records investigation and gates, not phase completion. Integrated
codecs/export/UI, Photoshop validation, performance and the
full local/remote acceptance sequence remain outstanding.

## Existing contracts

P7 completion evidence is the `p7-transform` annotation. Its final CI
`40402058829` and deployment `40402058879` passed at the starting head; the
inspected final suites contain 444 hardware and 329 software cases, each passing
once without skips or retries. P8 must establish its own final-head evidence.

The worker owns Rust documents, GPU resources, paging and candidate transactions.
React/zustand owns UI state; TypeScript may move byte chunks and file handles but
must not process document pixels. Every editing mutation uses commands and exact
undo/redo. Import/reopen constructs a complete candidate before document replacement
and begins a fresh history as already decided. Export never modifies samples,
history, selection, guide/crop metadata or the redo branch.

Keep exact straight RGBA8, including hidden RGB. Color conversion intentionally
changes RGB according to the source profile but preserves alpha, including zero
alpha; no premultiplication may erase hidden RGB. Untagged inputs continue to use
sRGB. Invalid/unsupported tagged input must produce an explicit outcome, never a
silent sRGB assumption. Do not treat arbitrary profile byte equality as evidence
of equivalent transforms. 16-bit processing remains P11; reject unsupported depth
before decoding/conversion rather than silently downconverting it.

Retained and visible extents are capped at 16,384 per side. Total logical CPU is
768 MiB, GPU 128 MiB, image files 128 MiB and streamed projects 4 GiB. Include old
document/history, candidate, compressed and decoded data, conversion scratch,
composites, thumbnails and bridge copies in admission. Logical capacities remain
distinct from process/browser/driver memory. All changed jobs need cancellation,
paging-failure, budget-refusal and preserved-current-document/redo coverage.

## Product decisions (answered 2026-10-27)

The user selected PSD import only, BMP/GIF first-frame/8-bit TIFF imports, embedded
sRGB ICC by default on export, and the original ZIP/LZ4 project container. HEIC
and PSD export are not selected. No local Photoshop/fixture corpus is available;
investigate public fixtures without treating that as a passed compatibility gate.
`.cgfoto`, fresh history on reopen, session-only selection and backward reading
remain existing decisions. Dependency and implementation investigation may proceed.

The selected ZIP/LZ4 format differs from both
the v1 single-raster container and the v2 framing/schema-4 streamed layered format.
A ZIP implementation must stream entries and bound directory/manifest sizes,
reject duplicate/unsafe names and unsupported/encrypted entries, enforce decoded
tile sizes and expansion bounds, preserve shared generations and validate CRCs
and all references before commit. Compression must not force all layers into RAM.
The current pager uses immutable raw tile offsets; compressed project tiles need
validated decompression into an immutable paged backing store or a revised bounded
backing abstraction. Do not reinterpret compressed offsets as raw tile locations.
Legacy projects/recovery must remain readable across any migration.

## Independent reader foundation

`project_adversarial.rs` exercises the existing legacy and streamed adapters with
every truncation and single-bit mutation of small valid files, 1,024 seeded
mutations of a real hidden-RGB tile stream, varying chunk boundaries, and valid
framing/checksums around invalid document metadata. It covers dimensions, DPI,
schema, blend space, canonical IDs, layer properties/content, missing tile
references, overflowed/out-of-bounds crop metadata and unknown fields. Valid
reads remain cold; full-budget admission refuses before candidate construction.
An empty layer tree is deliberately valid, matching Delete All Layers. The first
test draft assumed otherwise; that assertion was corrected after inspecting the
existing core contract. No reader production change was needed for these cases.
This deterministic corpus complements future coverage-guided fuzzing; it does not
claim exhaustive fuzzing or coverage of an unimplemented ZIP/PSD decoder.

`PsdPreflight::inspect` is allocation-free borrowed framing inspection for PSD v1,
8-bit RGB, valid sides/channels and bounded outer sections. It checks Pascal-name
and resource padding, borrows ICC/VersionInfo resources, rejects ambiguous duplicate
resources, and bounds the signed layer count (including `i16::MIN`). It explicitly
rejects unsupported version/depth/color mode. It does not decompress channels,
decode samples, validate every inner layer record or open a PSD into the editor.
Passing preflight must never be presented as Photoshop compatibility.

`cargo run -p fotoeditor-io --example inspect_psd -- PATH...` produces JSON framing
and resource metadata for candidate fixtures, explicitly marked `decoded: false`
and `photoshopValidated: false`. Native fixture inspection and synthetic preflight
tests do not replace the eventual worker/browser/desktop import checks.

## Codec investigation

The existing `image` 0.25.10 build enables PNG/JPEG/WebP. Its WebP encoder supports
lossless only, so it cannot satisfy the required lossy quality control alone.
qcms 0.3.0 is an MIT Rust candidate for ICC conversion. The proposed psd 0.3.5
uses MIT/Apache-2.0 but must not be adopted merely because it parses simple files:
the reviewed roadmap already identified missing mask metadata and RGBA8 depth
limitations. Inspect bounds, panic paths, compression, layer origin and masks
before choosing an extension or replacement.
Registry source inspection confirms that 0.3.5's layer-record reader explicitly
skips the layer mask data block (`sections/layer_and_mask_information_section/
mod.rs`, lines 397-399). Adopting it unchanged cannot satisfy editable mask import.

Registry inspection finds webp-rust 0.3.2 (MIT) and webpkit 0.1.0
(MIT OR Apache-2.0) as lossy encoder candidates needing correctness, memory,
WASM, cancellation and size evaluation. zenwebp 0.4.6 is AGPL-3.0-only OR a
commercial license and is ineligible under the existing policy. Neither WebP
candidate is added to Cargo.toml/lockfiles or shipped. Metadata research/download to the Cargo cache
does not constitute a dependency approval or license inventory addition.

Primary references: [image WebP encoder](https://docs.rs/image/0.25.10/image/codecs/webp/struct.WebPEncoder.html),
[qcms](https://github.com/FirefoxGraphics/qcms),
[psd](https://github.com/chinedufn/psd),
[Adobe PSD specification](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/),
[webp-rust](https://github.com/mith-mmk/webp-rust),
[webpkit](https://github.com/P4suta/webpkit).

## Photoshop fixture gate

Public candidate collections include
[psd-tools fixtures](https://github.com/psd-tools/psd-tools/tree/main/tests/psd_files)
and [psd fixtures](https://github.com/chinedufn/psd/tree/master/tests/fixtures).
Before vendoring, verify actual fixture redistribution terms and preserve notices.
Pin upstream revision and content SHA-256, source URL, author/provenance, Photoshop
creator/version evidence where available, dimensions/depth/color profile, expected
layer order/properties and the supported/unsupported feature outcome per file.
An embedded Photoshop writer string is evidence about file metadata, not proof
that our renderer matches Photoshop or a substitute for expected outputs.

The acceptance corpus must cover raster order, offsets/negative origins, Unicode
names, opacity/fill/visibility, every claimed blend, masks with independent bounds,
default coverage, disabled/inverted flags and partial coverage, groups/clipping
where claimed, raw/RLE/ZIP channel variants, alpha-zero hidden RGB, ICC, corrupt
lengths and unsupported 16/32-bit input. Validate actual Photoshop reference
composites and expected editable channel/mask samples. Record tolerance, alignment
and color settings. A self-written PSD roundtrip is supplementary evidence only.
If no suitable references exist, keep that acceptance gate open without `p8-files`.
Carry the separate P5 feather fixture obligation forward without `p5-select`.

The first inspection pins psd-tools revision
`0385aac27d6e28921516c7a045e2fe460d15f0ad`. Seven candidate files and their source
URLs, Git blob IDs, lengths and SHA-256 values are in ignored
`bench/results/p8-psd-candidates/provenance.json`. Six 8-bit files pass framing;
`16bit5x5.psd` is explicitly rejected. No files are vendored into the repository.
`creator-metadata.json` records raw metadata findings: group/hidden-layer fixtures
identify Photoshop CS3, layer_mask_data identifies CS5, blend-and-clipping
identifies CC 2017. mask-disabled has a Photoshop 2020 reader string but CS3 XMP
CreatorTool; retain both rather than inventing one authoritative creator version.
2layers has neither of those resources. None has a validated reference output yet.

## Export and file-host integration

Required PNG/JPEG/WebP exports run in Rust using bounded source/composition jobs.
Full document, active raster layer and selection export need explicit source
semantics and exact visible-window clipping, including odd tile edges and retained
outside samples. JPEG needs an explicit matte. PNG/lossless WebP must verify exact
decoded samples, including alpha-zero RGB where the codec supports preservation;
lossy alpha and RGB error must have separate assertions. Size preview is the actual
encoded output length with visible progress/cancellation, not a guessed estimate.

File pickers must run from user activation on the main thread; the worker receives
handles/chunks. Reusing a handle, Save As, cancelled pickers, revoked permission,
missing recent files, write/flush/readback failures and fallback downloads need
tests. Large saves must retain streaming and avoid large Blob/ArrayBuffer copies.
Recent entries must not request permission or open content without a user action.

## Performance and completion

The original target is a 20-layer 6000x4000 save under three seconds. Use twenty
distinct dense layers with nontrivial RGBA/hidden RGB and verify file/document
hashes, not sparse/shared duplicates. The existing bounded dense generator can
produce this fixture with 7,680 RGBA tiles (1,920 MiB padded authoritative samples).
That residency exceeds RAM admission and therefore requires paging throughout.
Measure user-observed save through completion/flush and validation; report codec,
copy, readback and optional composite/thumbnail components separately. Identify
cold/warm backing/composite and OS-cache conditions and actual storage. Never
claim the target passed using native encoding alone or omitting required work.

The initial generator/reader validates a 2,013,676,863-byte fixture at
`bench/results/fixtures/p8-layers-20-6000x4000.cgfoto`, SHA-256
`2133a854f4ff2b605a44ebe28652d872d5188cdfc47ae43822cd20289917a542`.
It contains twenty generated distinct dense layers, 7,680 tile records, and zero
resident sample bytes after native parse. This establishes fixture construction
and bounded metadata reading only. No browser save timing or under-three-seconds
result is established by `p8-save-fixture.log`.

For each integrated increment run applicable checks before a small conventional
commit on main. Full phase acceptance includes Rust formatting/native+WASM clippy/
tests, TS/ESLint/Vitest/Prettier, release build with the approved 2,000,000-byte gzip
WASM gate, complete dependency inventory, installed Chrome and Edge on hardware
WebGPU/WebGL2, Windows/WebView2, measured workload and exact-final-head remote
CI/deployment. Inspect raw artifacts for failures, skips and retries. P7's 24 MP
arbitrary rotation cost and P6's first-use/legacy brush limits remain documented.
Only publish `p8-files` after every applicable gate passes, then report shipped,
deferred and measured work, propose P9 and stop before starting it.

## Initial checkpoint verification (2026-10-27)

All 158 Cargo tests, native and WASM clippy, Rust/JS formatting, TypeScript,
ESLint, 16 Vitest cases, 451 dependency license expressions and production build
pass. WASM remains 3,820,283 bytes / gzip9 1,423,577, with no added dependencies.
The applicable GPU/viewport/history/layer-project/recovery/crop browser subset
passes all 84 cases (42 Chrome, 42 Edge), covering hardware WebGPU/WebGL2, each
on one attempt with zero errors/skips/retries. This is a regression subset for
unintegrated Rust I/O APIs; it does not claim the full P8 acceptance suite exists
or that a PSD can be opened in the editor. The rebuilt Windows shell and existing
expanded WebView2 smoke pass both NVIDIA hardware APIs. Raw evidence is
`bench/results/p8-foundation-verification.json` and the associated logs/reports.
Remote checkpoint CI/deployment is separate from eventual final-phase verification.

The foundation commit `b881a1b` passes CI `40413241817` and deployment
`40413241805`. Its inspected artifact has all 329 software browser cases passing
on one attempt, zero errors/skips/retries, and both software WebView2 backends.
This evidence applies to that committed foundation, not subsequent working-tree
codec/ZIP integration.

## Integrated codec/ZIP checkpoint

The working tree implements BMP, GIF first-frame and classic 8-bit TIFF imports,
RGB/grayscale matrix/TRC ICC conversion through qcms 0.3.0, and canonical generated
sRGB ICC embedding in PNG exports. Untagged samples remain unchanged. Canonical
working-space exports avoid a needless quantized conversion on reopen. Alpha and
hidden RGB survive. LUT/CMYK profiles and BigTIFF are refused explicitly. TIFF
uses the first image, with non-ICC metadata capped at 1 MiB and ICC at 4 MiB.
PNG inflation/header metadata is capped before construction; JPEG/WebP/GIF/TIFF
metadata gets bounded preflight. Color/metadata scratch reserves 32 MiB. A
generated 257x259 linear-profile fixture compares against LittleCMS 2.19 via
Pillow ImageCms with maximum RGBA errors `[2, 1, 0, 0]` and unchanged alpha.

New ZIP/LZ4 project files retain `.cgfoto`. The ZIP profile uses stored entries,
bounded raw LZ4 blocks with exact decoded tile lengths, schema-4 `manifest.json`,
64-pixel PNG thumbnail and visible-window `composite.png`. It excludes ZIP64,
encryption, descriptors, extras and comments. Headers/directories must agree;
names are safe/unique, extents contiguous, and CRCs/entry lengths/expansion/file
limits are checked before a complete candidate can publish. ZIP scratch is
32 MiB plus the existing 16 MiB document-adapter reserve. PNG previews get bounded
signature/header/chunk/CRC validation; they are not decoded into authoritative
samples when opening. The document is rebuilt from exact tile data and renders
its own composite. Download fallback uses a file-backed OPFS snapshot, avoiding
a whole-project ArrayBuffer/Blob allocation. Those temporary download packs
remain in the owning session's directory until abandoned-session cleanup.

Opening converts bounded compressed tiles into a flushed immutable raw backing
pack, then runs the existing semantic document validator before replacement.
Both encoded archive and expanded backing retain the 4 GiB limit. Legacy v1 and
raw-stream schemas 2/3/4 remain readable; recovery continues to use the existing
validated raw generations. Cancellation, failed storage and invalid input release
only their candidate backing, preserving stores referenced by document/history.
The PNG export output/bridge reservation remains visible while paging hydrates
sources, rather than allowing those allocations to consume its admitted capacity.

The candidate passes all 168 Rust tests, native/WASM clippy, TypeScript,
ESLint, formatting, 16 Vitest cases and 458 license expressions. All 64 focused
hardware browser cases pass once across Chrome/Edge and both APIs, with no errors,
skips or retries. Expanded hardware WebView2 passes new imports, ICC refusal,
new ZIP files and the existing editing/crop/streamed roundtrip regressions on both
NVIDIA APIs. The final 32 MiB metadata-reserve adjustment passes those code checks,
another 64 hardware GPU/viewport/format cases on one attempt without any failures,
skips or retries, and the rebuilt expanded WebView2 smoke on both hardware APIs.
Raw evidence is consolidated in `bench/results/p8-zip-format-checkpoint.json`;
logs distinguish the earlier history/crop-focused and final viewport-focused runs.
Independent Python
zipfile, python-lz4 4.4.5 and Pillow validate generated archive CRCs, all retained
RGBA samples exactly (`[0, 0, 0, 0]` maximum error), crop metadata and PNG previews.
Evidence: `bench/results/p8-zip-*`, `p8-icc-reference.json` and
`p8-zip-independent.json`.

The initial standard release builds failed the former 1,500,000-byte gzip gate. The first
codec build is 1,551,251 bytes; the integrated ZIP build is 1,571,997 bytes, and
the final memory-guard build is 1,572,075 bytes (4,279,499 raw). A
shrink-level-one probe saves only 2,661 bytes from the first build and is not
adopted. The user subsequently approved a 2,000,000-byte P8 gate on 2026-10-27.
The standard production build passes that enforced limit: 4,280,109 raw bytes,
gzip9 1,572,150. Native codecs/ICC and native ZIP serialization are committed as
`d2ae624` and `7556b05`; the separately verified worker/UI integration passes
168 Rust tests, native/WASM clippy, formatting, TypeScript, ESLint, 16 Vitest
cases and 458 licenses. All 72 hardware Chrome/Edge format/project/history/crop
cases pass once, zero failures/skips/retries/global errors. The rebuilt shell's
expanded WebView2 smoke passes both NVIDIA APIs. Its initial attempt caught a
test race: direct RPC completion precedes React's idle snapshot; file changes
must wait for the Open control to become enabled. The corrected smoke waits
for that actual state rather than adding a sleep. Evidence is
`bench/results/p8-zip-integration-*`.
Earlier functional web/desktop builds do not retrospectively turn the
failed size checks into passes. Final phase acceptance still requires all remaining
features, measurements and exact-head local/remote gates.

The subsequent PSD/file-access checkpoint passes 171 Rust tests and 146 focused
single-attempt hardware browser cases, plus rebuilt WebView2 on both hardware
APIs (13 public PSD imports per backend). Production WASM is 4,322,993 bytes /
gzip9 1,588,906. P8-PSD.md records independent reference results and unsupported
features. Worker FileReader admission/cancellation precedes document replacement.
File System Access saves reuse granted project handles; recent handles persist
in a ten-entry IndexedDB catalog, with permission requests only on explicit use.
Fallback File inputs cannot provide reusable filesystem handles.

Remote CI 40467804466 exposed the desktop race again: a hidden input being enabled
does not prove React observed completion of a direct crop RPC. The updated smoke
waits for the final revealed dimensions/idle snapshot and the visible Open control.
Current hardware verification passes; new-head remote verification is pending.

The first actual dense 20-layer 24 MP Chrome/WebGPU probe takes 65.407375,
65.599475 and 66.280645 seconds per save, each 2,123,367,314 bytes. Logical CPU
peaks at 611,624,942 bytes; GPU is 4,591,760 bytes. The D: device identifies as
WDC WD1000DHTZ-60N21V0 through RAID. OS caches were not cleared. The source raw
project is 2,013,676,863 bytes, SHA-256
`2133a854f4ff2b605a44ebe28652d872d5188cdfc47ae43822cd20289917a542`;
the measured ZIP SHA-256 is
`9978218f56d7d25ba744bb603f118e9401eb8943b927a15c9b4643c75d9bb1c4`.
These file hashes identify evidence, not equivalent canonical document hashes.
See `bench/results/p8-save-performance-probe.json`. The under-three-second
criterion fails. At that checkpoint no exception was approved; the later user
decision in DECISIONS.md accepts the measured P8 limit and defers speed to P12.

The subsequent export candidate adds the dialog and bounded PNG/JPEG/WebP sources
and codecs; P8-EXPORT.md records execution/memory contracts and independent codec
checks. It passes 173 Rust tests, 459 licenses, 124 focused hardware browser cases
and rebuilt WebView2 on both hardware APIs. A further file-cancellation lifetime
fix passes 28 export/file-access cases, including destination preservation after
real writes followed by cancellation/failure. Release WASM is 4,440,303 bytes /
gzip9 1,638,683. Project output batching/profiling is implemented; new measurements
remain separate from the earlier 65-second probe.

scripts/project-reference.py independently decodes every stored LZ4 generation,
validates ZIP/raw framing CRCs and hashes canonical document metadata with each
generation reference replaced by its complete sample SHA-256. Legacy omitted
canvas/guides defaults are normalized, and file schema is not document data.
The 7,680-tile source and saved archive match canonical SHA-256
`7e7073f7c09310418c777f351369f4df961d22429a01ccafe9b8165e65990db6`.
The earlier audit draft treated absent/explicit defaults differently; fixing
that audit required no product/sample change. Evidence is
`bench/results/p8-large-canonical-reference.json`.

Still required: save performance investigation, full regression/current desktop
checks and final-head remote gates. PSD export and HEIC remain unselected.

## Final color/measurement audit

The image BMP decoder skips V4/V5 color fields. A bounded adapter now extracts
embedded V5 RGB ICC data using DIB-relative offsets, rejects overlap/truncation,
caps profiles at 4 MiB and routes them through the existing qcms conversion.
Explicit sRGB/Windows-sRGB definitions are accepted; linked, calibrated and unknown
definitions are refused rather than silently interpreted as sRGB. No external
profile path is followed. Field layout follows Microsoft's
[BITMAPV5HEADER](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapv5header).
The added native test covers alpha-zero RGB, all alpha extremes, linear conversion
and invalid extents/color definitions. All 174 Rust tests and native/WASM clippy
pass. Independent Pillow/LittleCMS 2.19 compares both 257x259 PNG/BMP linear inputs
at maximum RGBA error `[1, 1, 0, 0]`, with exact alpha, including hidden RGB.
Production WASM is 4,443,265 bytes / gzip9 1,639,949. All 40 focused hardware
format/export browser cases pass once with no errors/skips/retries. Rebuilt
WebView2 passes both NVIDIA APIs, including the tagged BMP and refusal case.
Evidence: `bench/results/p8-bmp-icc-*`.

Screenshot inspection exposed a benchmark-harness mistake: it timed genuine
document I/O but left the fixed diagnostic texture onscreen. Those reports/logs
and archives are retained as superseded `p8-*-diagnostic-*` evidence. Final
benchmarks request `document=1` and require visible tiles with completed
refinement before timing. Normal-document runs must complete before acceptance.
The user's explicit speed-target deferral to P12 remains recorded in DECISIONS.md.

Independent audits of both large encoded variants match the same canonical
document hash. Their sole ZIP entry difference is `composite.png`: maximum RGB
backend error is one, alpha is exact, and its encoded length differs by one byte.
Manifest, thumbnail and all authoritative tile payloads agree. This does not
substitute preview comparisons for document sample validation.
