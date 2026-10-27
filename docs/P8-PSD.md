# P8 raster PSD reader and evidence

The native reader handles PSD v1, 8-bit RGB raster layers with raw, PackBits RLE,
ZIP and ZIP prediction channels. It preserves Unicode names, layer order,
visibility, opacity/fill, supported blend keys, clipping flags, locks, nested
groups and editable raster masks. Negative/off-canvas layer and mask bounds
expand the retained tile extent; the original canvas remains a visible window.
RGB samples remain straight, including hidden RGB. Embedded supported matrix/TRC
ICC profiles convert to sRGB, preserving alpha.

The checked parser borrows sections before allocating channels. It bounds file,
records, metadata, dimensions, group depth, channel extents and decompression.
Raw/RLE lengths must match; ZIP must end at exactly the declared plane with no
extra output or trailing stream. Decoding advances by at most 16 scanlines, or
one ICC tile, per step. A private candidate reserves its complete pixel/mask
allocation, input, metadata/color scratch and first-composite surface/stack
before construction. Failure or cancellation cannot publish partial layers.

The supported subset explicitly refuses PSB, non-RGB/16/32-bit data, missing
raster layers, vector/type/smart/adjustment content, layer effects, unsupported
blends, Blend If/knockout settings, vector/real masks and mask density/feather
parameters. Arbitrary unknown rendering tags are refused. Known mask-display
overlays, identifiers and cached-compositor provenance do not become pixels.
Merged preview RGB never substitutes for editable layer samples.

## Public corpus

`tests/fixtures/psd/sources.json` records 13 unmodified public fixtures, pinned
upstream revisions, lengths, SHA-256s and source URLs; MIT notices are preserved.
`oracle.json` records actual VersionInfo strings rather than inferring an author.
The identifiable references include Photoshop CS6, CC 2019 and older Photoshop
versions. The visibility fixture has no VersionInfo and is not attributed to a
Photoshop version. This is a scoped corpus, not a general PSD compatibility claim.

Independent psd-tools 1.26.0 decodes raw layer/mask channels. LittleCMS 2.19 through
Pillow converts source ICC profiles independently of qcms. Twelve fixtures match
the resulting RGBA samples exactly; one tagged semi-transparent fixture differs
by one red value in one layer. All alpha and mask samples match exactly. The
existing two-value ICC tolerance is used only for tagged RGB conversion;
untagged samples and alpha remain exact.

Photoshop's merged RGB is stored over a white matte. Comparing unassociated RGB
after unmatting at alpha 1 amplifies byte quantization and is not a useful visual
tolerance. Tests therefore compare the original stored white-matte RGB and the
stored alpha separately. The measured corpus differs by at most one RGB value
and one alpha value. Test limits are two RGB values and one alpha value. No
psd-tools recomposite or FotoEditor roundtrip generates those expectations.

Native tests also cover all four compression methods with hidden RGB, disabled
masks and negative bounds, every truncation of small valid fixtures, mutations,
poisoned failed jobs, invalid channel framing and full-budget admission refusal.
`scripts/psd-reference.py --check` verifies source hashes and independently
regenerates the immutable oracles without changing them. Cargo tests do not need
Python, internet access or Photoshop.

Browser/worker/Desktop integration and the complete P8 acceptance checks remain
separate gates. P5's Photoshop feather comparison remains pending: these mask
fixtures do not validate FotoEditor's feather operation.
