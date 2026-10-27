# PSD regression corpus

Unmodified PSDs from the pinned public repositories in `sources.json`. Every
source has its upstream URL, revision, length and SHA-256. Upstream MIT notices
are preserved in the three `LICENSE-*.txt` files. These fixtures are test assets;
they are not copied into the production web/desktop bundle.

`oracle.json`, `*.layer-*.png` and `*.reference.png` are independently generated
by `scripts/psd-reference.py` using psd-tools 1.26.0 and LittleCMS 2.19 through
Pillow. The layer/mask channels are decoded directly. The composite references
read Photoshop's stored merged RGB and alpha, not a library recomposite.
Photoshop stores merged RGB over white; the test compares that white matte and
the alpha plane separately, avoiding amplified unmatting error at tiny alpha.

The `photoshop`/`writer` fields quote each file's VersionInfo resource. An absent
VersionInfo is recorded as null rather than attributed to Photoshop. The corpus
covers raster order, Unicode names, negative bounds, hidden layers/groups,
opacity, normal/multiply and pass-through/isolated groups, raster mask samples,
and empty default masks. It does not validate every supported blend against
Photoshop or establish compatibility with arbitrary PSD features.

Native Cargo tests verify the immutable oracles without Python. To regenerate
or independently recheck, install the optional MIT psd-tools/attrs QA tools in
`.tools/p8-reference`, then run:

```text
python scripts/psd-reference.py --check
```

Remove `--check` only when deliberately updating the independently generated
expectations. Do not replace expectations with FotoEditor's own output.
