"""Independent PSD channel/composite oracles; requires psd-tools 1.26 and Pillow.

Run from the repository root. Native QA tools may be installed in the ignored
.tools/p8-reference directory. --check verifies without changing tracked oracles.
No FotoEditor code or recompositing library creates the Photoshop reference.
"""
from pathlib import Path
import hashlib
import io
import json
import sys
import zlib

sys.path.insert(0, str(Path(".tools/p8-reference").resolve()))
import numpy as np
import psd_tools
from psd_tools import PSDImage
from PIL import Image, ImageCms

ROOT = Path("tests/fixtures/psd")
CHECK = "--check" in sys.argv


def write(path, data):
    if CHECK:
        if path.read_bytes() != data:
            raise ValueError(f"Stale independent PSD oracle: {path}")
    else:
        path.write_bytes(data)


def generate(source):
    path = ROOT / source["file"]
    if hashlib.sha256(path.read_bytes()).hexdigest() != source["sha256"]:
        raise ValueError(f"PSD source hash changed: {path}")
    psd = PSDImage.open(path)
    profile = psd.image_resources.get_data(1039)

    def srgb(image):
        image = image.convert("RGBA")
        alpha = image.getchannel("A")
        if profile:
            image = ImageCms.profileToProfile(
                image.convert("RGB"), ImageCms.ImageCmsProfile(io.BytesIO(profile)),
                ImageCms.createProfile("sRGB"), renderingIntent=0,
            ).convert("RGBA")
            image.putalpha(alpha)
        return np.array(image)

    boxes = [psd.bbox]
    layers = list(psd.descendants())
    for layer in layers:
        if layer.kind == "pixel" and layer.width and layer.height:
            boxes.append(layer.bbox)
        if layer.mask and layer.mask.width and layer.mask.height:
            boxes.append(layer.mask.bbox)
    x0 = min(0, *(box[0] for box in boxes))
    y0 = min(0, *(box[1] for box in boxes))
    width = max(psd.width, *(box[2] for box in boxes)) - x0
    height = max(psd.height, *(box[3] for box in boxes)) - y0
    expected = []
    for layer in layers:
        item = dict(name=layer.name, kind=layer.kind, visible=layer.visible,
                    opacity=layer.opacity, fill=layer._record.tagged_blocks.get_data(b"iOpa", 255),
                    blend=layer.blend_mode.value.decode("ascii"), clipped=layer.clipping,
                    sourceBounds=layer.bbox)
        item["parentIndex"] = None if layer.parent is psd else layers.index(layer.parent)
        if layer.kind == "pixel":
            samples = np.zeros((height, width, 4), dtype=np.uint8)
            image = layer.topil(apply_icc=False)
            if image is not None:
                x, y = layer.left - x0, layer.top - y0
                samples[y:y + layer.height, x:x + layer.width] = srgb(image)
            item["rgbaCrc32"] = zlib.crc32(samples.tobytes())
            reference = path.stem + f".layer-{len(expected)}.png"
            encoded = io.BytesIO()
            Image.fromarray(samples).save(encoded, format="PNG")
            write(ROOT / reference, encoded.getvalue())
            item["rgbaReference"] = reference
        if layer.mask:
            mask = layer.mask
            samples = np.full((height, width), mask.background_color, dtype=np.uint8)
            if mask.width and mask.height:
                image = np.array(mask.topil())
                x, y = mask.left - x0, mask.top - y0
                samples[y:y + image.shape[0], x:x + image.shape[1]] = image
            item["mask"] = dict(enabled=not mask.disabled, default=mask.background_color,
                                crc32=zlib.crc32(samples.tobytes()), sourceBounds=mask.bbox)
        expected.append(item)
    # Photoshop's merged RGB is stored over a white matte. Read its original
    # channels, not psd-tools' recomposite, and preserve the stored alpha plane.
    matte = Image.merge("RGB", [psd.topil(channel=i, apply_icc=False) for i in range(3)])
    rgba = srgb(matte)
    rgba[:, :, 3] = np.array(psd.topil(apply_icc=False).getchannel("A")) if psd.topil(apply_icc=False).mode == "RGBA" else 255
    png = io.BytesIO()
    Image.fromarray(rgba).save(png, format="PNG")
    reference = path.stem + ".reference.png"
    write(ROOT / reference, png.getvalue())
    version = psd.image_resources.get_data(1057)
    return dict(file=source["file"], sha256=source["sha256"],
                photoshop=None if version is None else version.reader,
                writer=None if version is None else version.writer,
                hasStoredComposite=psd.has_preview(), width=width, height=height,
                canvas=dict(x=-x0, y=-y0, width=psd.width, height=psd.height),
                layers=expected, compositeReference=reference,
                channelRgbTolerance=2 if profile else 0,
                whiteMatteRgbTolerance=2, alphaTolerance=1)


results = [generate(source) for source in json.loads((ROOT / "sources.json").read_text(encoding="utf-8"))]
report = dict(psdToolsVersion=psd_tools.__version__, littleCmsVersion=ImageCms.core.littlecms_version,
              method="Raw layer channels/masks and Photoshop-stored white-matte composite; no recompositing oracle.", fixtures=results)
write(ROOT / "oracle.json", (json.dumps(report, indent=2, ensure_ascii=False) + "\n").encode("utf-8"))
print(f"{'Verified' if CHECK else 'Generated'} {len(results)} independent PSD oracles")
