"""Decode the Rust QA outputs with independent Pillow/libjpeg/libwebp."""
import json
import sys
from pathlib import Path
from PIL import Image, features

folder = Path(sys.argv[1])
source = (folder / 'source.rgba').read_bytes()
profile = (folder / 'srgb.icc').read_bytes()
report = {'decoder': {'pillow': Image.__version__, 'jpeg': features.version('jpg'), 'webp': features.version('webp')}, 'width': 65, 'height': 33, 'results': []}
for path in sorted(folder.glob('*')):
    if path.suffix not in ('.webp', '.jpg'):
        continue
    with Image.open(path) as image:
        assert image.size == (65, 33)
        tagged = path.name.startswith('tagged.')
        assert image.info.get('icc_profile') == (profile if tagged else None)
        decoded = image.convert('RGBA').tobytes()
        alpha_error = max(abs(a-b) for a,b in zip(decoded[3::4], source[3::4]))
        rgba_error = max(abs(a-b) for a,b in zip(decoded, source))
        if path.suffix == '.webp':
            assert alpha_error == 0
        if 'lossless' in path.name:
            assert rgba_error == 0
        if path.suffix == '.jpg':
            assert set(decoded[3::4]) == {255}
            assert image.info.get('dpi') == (300, 300)
        report['results'].append({'file': path.name, 'bytes': path.stat().st_size, 'icc': tagged, 'maxAlphaError': alpha_error, 'maxRgbaDifferenceFromStraightSource': rgba_error, 'note': 'JPEG intentionally mattes transparency; lossy RGB differences are not an exact-sample acceptance comparison.' if path.suffix == '.jpg' else 'Alpha exact; lossless mode also requires every RGB byte, including hidden samples, to match.'})
assert len(report['results']) == 6
(folder.parent / 'p8-export-independent.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
