"""Independent LittleCMS conversion for the PNG/BMP linear-profile QA pair."""
import io
import json
import math
import struct
import sys
from pathlib import Path
from PIL import Image, ImageCms

folder = Path(sys.argv[1])
profile = (folder / 'p8-linear.icc').read_bytes()
input_profile = ImageCms.ImageCmsProfile(io.BytesIO(profile))
output_profile = ImageCms.createProfile('sRGB')
report = {'decoder': 'Pillow ' + Image.__version__, 'colorEngine': 'LittleCMS ' + ImageCms.core.littlecms_version, 'results': []}
source_pixels = None
for name in ['p8-icc-linear.png', 'p8-icc-linear.bmp', 'p8-associated.tiff']:
    path = folder / name
    with Image.open(path) as image:
        source = image.convert('RGBA')
        associated = path.suffix == '.tiff'
        if associated:
            assert image.tag_v2[338] == (1,)
            assert image.tag_v2[259] == 1 and image.tag_v2.get(284, 1) == 1
            data = path.read_bytes()
            stored = b''.join(data[offset:offset + length] for offset,length in zip(image.tag_v2[273], image.tag_v2[279]))
            assert len(stored) == source.width * source.height * 4
            straight = bytearray(stored)
            for at in range(0, len(straight), 4):
                alpha = straight[at + 3]
                if alpha:
                    for channel in range(3):
                        straight[at + channel] = min(255, math.floor(stored[at + channel] * 255.0 / alpha + 0.5))
            source = Image.frombytes('RGBA', source.size, bytes(straight))
        elif source_pixels is None:
            source_pixels = source.tobytes()
        else:
            assert source.tobytes() == source_pixels
        if path.suffix == '.bmp':
            data = path.read_bytes()
            word = lambda at: struct.unpack_from('<I', data, at)[0]
            assert word(14) == 124 and word(70) == 0x4D424544
            start, size = 14 + word(126), word(130)
            assert data[start:start + size] == profile
        else:
            assert image.info['icc_profile'] == profile
        reference = ImageCms.profileToProfile(source, input_profile, output_profile, renderingIntent=0, outputMode='RGBA').tobytes()
        with Image.open(folder / (name + '.expected.png')) as result:
            assert result.size == source.size
            actual = result.convert('RGBA').tobytes()
        maximum = [max(abs(a-b) for a,b in zip(actual[channel::4], reference[channel::4])) for channel in range(4)]
        assert max(maximum[:3]) <= 2 and maximum[3] == 0
        assert actual[3::4] == source.tobytes()[3::4]
        report['results'].append({'file': name, 'width': source.width, 'height': source.height, 'maxRgbaError': maximum, 'alphaExact': True, 'includesHiddenRgb': 0 in source.tobytes()[3::4], 'associatedAlpha': associated, 'reference': 'Floating-point inverse association rounded to nearest RGBA8, then independent LittleCMS.' if associated else 'Independent Pillow/LittleCMS over straight samples.'})
(folder.parent / 'p8-icc-independent.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
