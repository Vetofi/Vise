#!/usr/bin/env python3
try:
    from . import audio_harness as h
except ImportError:
    import audio_harness as h


def fixtures():
    data = h.RESOURCES / 'data/minecraft'
    biomes = sorted('minecraft:' + p.stem for p in (data / 'worldgen/biome').glob('*.json'))
    dimensions = sorted('minecraft:' + p.stem for p in (data / 'dimension_type').glob('*.json'))
    return [f'{d} {b} {t}' for d in dimensions for b in biomes for t in (0, 13000)]


def main():
    lines = fixtures()
    matched, report = h.run_harness(
        'audio-attributes', 'VanillaAudioAttributesOracle', 'audio_attributes_fixture', ''.join(l + '\n' for l in lines),
        extra_sources=[h.lab.ROOT / 'crates/vise-client/src/music.rs', h.lab.ROOT / 'crates/vise-client/src/ambience.rs'],
        rust_args=[h.RESOURCES],
        scope='Vanilla EnvironmentAttributeSystem values of audio/background_music (with BackgroundMusic.select), audio/music_volume and '
              'audio/ambient_sounds for every dimension type x biome at two times of day, and the Musics constants.')
    print(f'{matched:,} audio attribute lines match the pinned data ({len(lines):,} cases): {report}', flush=True)


if __name__ == '__main__':
    main()
