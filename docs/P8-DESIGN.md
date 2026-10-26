# P8 design and validation contracts

Started 2026-10-27 on clean main at `86043ed6e94cbf616dcb68c43e0ebb570029ae5f`.
The user explicitly authorized P8 in a fresh session without worktrees or agents.
AGENTS.md, PLAN.md, PROGRESS.md and DECISIONS.md were read before work. The
complete original P8 requirements and acceptance were read on printed pages
16-17, both extracted text and rendered images. The unchanged source is
`C:/Users/msada/OneDrive/Desktop/Raster Editor — Implementation Plan.pdf`, SHA-256
`83c7e9f904a7bb97d13e8f3ca87f2024e6760627c3a568c6c0065cdf861c62ff`.
Rendered review pages are ignored evidence under `bench/results/p8-roadmap/`.

This document records investigation and gates, not phase completion. Product
answers, integrated codecs/export/UI, Photoshop validation, performance and the
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

## Product decisions awaiting answers

The four unresolved product choices are PSD export scope, additional imports,
export ICC default and project container direction. The fifth question establishes
available Photoshop/corpus evidence. `.cgfoto`, fresh history on reopen,
session-only selection and the existing backward-reading promise already apply.
No new format dependency is selected while dependent scope remains unanswered.

The source ZIP/LZ4 proposal requires an explicit decision: it differs from both
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
commercial license and is ineligible under the existing policy. None is added to
Cargo.toml/lockfiles or shipped. Metadata research/download to the Cargo cache
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
tests, TS/ESLint/Vitest/Prettier, release build with the current 1,500,000-byte gzip
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
