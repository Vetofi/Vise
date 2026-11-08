#!/usr/bin/env python3
import json
import struct

try:
    from . import oracle_harness as oh
except ImportError:
    import oracle_harness as oh

NAME = 'mipmap-parity'


def size(png):
    with open(png, 'rb') as f:
        head = f.read(24)
    return struct.unpack('>II', head[16:24])


def make_input(build):
    root = oh.lab.RESOURCES
    textures = root / 'assets/minecraft/textures'
    names = []
    for folder, every in (('block', 1), ('item', 7)):
        for i, png in enumerate(sorted((textures / folder).glob('*.png'))):
            if i % every:
                continue
            meta = png.with_name(png.name + '.mcmeta')
            if meta.is_file() and 'animation' in json.loads(meta.read_text(encoding='utf-8')):
                continue
            w, h = size(png)
            if w != h or w & (w - 1) or w != 16:
                continue
            names.append(f'minecraft:{folder}/{png.stem}')
    path = build / 'sprites.txt'
    path.write_text('\n'.join([str(root)] + names) + '\n', encoding='utf-8')
    return path, len(names)


if __name__ == '__main__':
    oh.differential(NAME, 'VanillaMipmapOracle', 'mipmap_probe', make_input,
                    'MipmapGenerator.generateMipLevels (solidify, fillEmptyAreasWithDarkColor, meanLinear, '
                    'darkenedAlphaBlend, alphaTestCoverage, scaleAlphaToCoverage) on static 16x16 block and item sprites '
                    'with their .mcmeta texture settings',
                    sources=[oh.lab.ROOT / 'crates/vise-client-render/src/mipmap.rs',
                             oh.lab.ROOT / 'crates/vise-client-render/src/sprites.rs'])
