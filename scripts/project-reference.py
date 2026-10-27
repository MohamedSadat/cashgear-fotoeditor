"""Independent bounded canonical comparison of raw and ZIP/LZ4 projects (QA only)."""
import argparse
import hashlib
import json
import struct
import sys
import zlib
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / '.tools/p8-reference'))
import lz4.block


def digest(data):
    return hashlib.sha256(data).hexdigest()


def exact(file, size):
    data = file.read(size)
    if len(data) != size:
        raise ValueError('Truncated project reference')
    return data


def canonical(manifest, tiles):
    # Absent legacy defaults and an explicitly serialized current default have
    # the same document meaning. Schema is a file version, not document data.
    assert manifest.pop('schema', 2) in (2, 3, 4)
    manifest.setdefault('canvas', None)
    manifest.setdefault('guides', [])
    used = set()

    def references(entries):
        for entry in entries:
            generation = str(entry['generation'])
            entry['generation'] = tiles[generation]
            used.add(generation)

    def nodes(layers):
        for layer in layers:
            if layer['mask']:
                layer['mask'].setdefault('default', 255)
                references(layer['mask']['tiles'])
            content = layer['content']
            if content['kind'] == 'raster':
                references(content['tiles'])
            elif content['kind'] == 'group':
                nodes(content['children'])

    nodes(manifest['layers'])
    if used != set(tiles):
        raise ValueError('Unreferenced or missing sample generations')
    return digest(json.dumps(manifest, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode())


def raw(path):
    tiles = {}
    checksum = 0
    total = 0
    count = 0
    with path.open('rb') as file:
        signature = exact(file, 12)
        assert signature == b'CGFOTO\r\n\x02\0\0\0'
        checksum = zlib.crc32(signature)
        total += 12
        manifest = None
        while True:
            prefix = exact(file, 20)
            kind, generation, length, crc = struct.unpack('<IQII', prefix)
            if kind == 255:
                assert generation == 0 and length == 16
                footer = exact(file, length)
                assert zlib.crc32(footer) == crc
                records, size, whole_crc = struct.unpack('<IQI', footer)
                assert records == count and size == total + 36 and whole_crc == checksum
                break
            assert length <= 1048576
            payload = exact(file, length)
            assert zlib.crc32(payload) == crc
            checksum = zlib.crc32(payload, zlib.crc32(prefix, checksum))
            total += 20 + length
            count += 1
            if kind == 1:
                assert manifest is None and generation == 0
                manifest = json.loads(payload)
            else:
                assert kind in (2, 3) and length == (262144 if kind == 2 else 65536)
                assert str(generation) not in tiles
                tiles[str(generation)] = digest(payload)
        assert file.read(1) == b''
    return {'canonicalSha256': canonical(manifest, tiles), 'tiles': len(tiles), 'bytes': path.stat().st_size}


def archive(path):
    tiles = {}
    with zipfile.ZipFile(path) as file:
        entries = file.infolist()
        assert len(entries) == len(set(e.filename for e in entries))
        manifest = json.loads(file.read('manifest.json'))
        for entry in entries:
            assert entry.compress_type == zipfile.ZIP_STORED
            if entry.filename.startswith('tiles/'):
                assert entry.file_size <= 1048576
                payload = file.read(entry)
                expected = 262144 if '.rgba.' in entry.filename else 65536
                assert struct.unpack('<I', payload[:4])[0] == expected
                decoded = lz4.block.decompress(payload)
                assert len(decoded) == expected
                generation = entry.filename.split('/')[1].split('.')[0]
                assert generation not in tiles
                tiles[generation] = digest(decoded)
            else:
                assert entry.filename in ('manifest.json', 'thumbnail.png', 'composite.png')
                # Reading each stream to EOF validates its independent ZIP CRC.
                with file.open(entry) as stream:
                    while stream.read(1048576):
                        pass
    return {'canonicalSha256': canonical(manifest, tiles), 'tiles': len(tiles), 'bytes': path.stat().st_size}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('source', type=Path)
    parser.add_argument('saved', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'method': 'Independent Python zipfile CRC and python-lz4 decode; SHA-256 of canonical JSON document metadata with legacy canvas/guides defaults applied, excluding file schema version, and generation references replaced by SHA-256 of complete exact tile samples. All retained/padded RGBA and mask bytes participate; no composite substitutes for layers.', 'source': raw(args.source), 'saved': archive(args.saved)}
    report['equal'] = report['source']['canonicalSha256'] == report['saved']['canonicalSha256']
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    if not report['equal']:
        raise SystemExit('Canonical document mismatch')
