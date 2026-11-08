#!/usr/bin/env python3
import random

try:
    from . import lab, oracle_harness as harness
except ImportError:
    import lab, oracle_harness as harness


def make_cases(build):
    path = build / 'cases.txt'
    rng = random.Random(0x46495245)
    seeds = [0, 1, -1, 42, 2**63 - 1, -2**63] + [rng.randrange(-2**63, 2**63) for _ in range(20000)]
    path.write_text(''.join(f'{s}\n' for s in seeds))
    return path, len(seeds)


def main():
    scope = ('The real private FireChargeItem.playSound on a ServerLevel stand-in with a seeded LegacyRandomSource: '
             'the sound event, volume, pitch ((nextFloat - nextFloat) * 0.2 + 1) and the random afterwards. '
             'FireChargeItem.useOn\'s choice between lighting and placing fire is not exercised.')
    harness.differential(
        'rust-fire-starter-sound-parity', 'VanillaFireStarterSoundOracle', 'fire_starter_sound_probe', make_cases,
        scope, sources=[lab.ROOT / 'crates/vise-items/src/items.rs'])


if __name__ == '__main__':
    main()
