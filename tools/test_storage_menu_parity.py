#!/usr/bin/env python3
import argparse
try:
    from . import interaction_lab as lib
except ImportError:
    import interaction_lab as lib

COLOURS = ['white', 'orange', 'magenta', 'light_blue', 'yellow', 'lime', 'pink', 'gray', 'light_gray', 'cyan',
           'purple', 'blue', 'brown', 'green', 'red', 'black']
BLOCKS = (['chest', 'trapped_chest', 'barrel', 'dispenser', 'dropper', 'hopper', 'shulker_box']
          + [f'{c}_shulker_box' for c in COLOURS]
          + [f'{w}copper_chest' for w in ('', 'exposed_', 'weathered_', 'oxidized_')]
          + [f'waxed_{w}copper_chest' for w in ('', 'exposed_', 'weathered_', 'oxidized_')])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--limit', type=int, default=4)
    args = parser.parse_args()
    work = lib.work_dir('storage-menu-parity')
    vanilla = lib.Vanilla(work)
    vanilla.compile('ParityData', 'VanillaStorageMenuOracle')
    cases = ['minecraft:' + b for b in BLOCKS]
    (work / 'cases.txt').write_text(''.join(c + '\n' for c in cases), encoding='utf-8')
    vanilla.run('VanillaStorageMenuOracle', work / 'cases.txt', work / 'native.txt')
    lib.cargo_example('vise-client', 'storage_menu_fixture', work / 'cases.txt', work / 'rust.txt')
    errors = sum(line == 'error' for line in (work / 'native.txt').read_text(encoding='utf-8').split('\n'))
    if errors:
        print(f'{errors} cases fail in vanilla')
    total = lib.compare(cases, work / 'native.txt', work / 'rust.txt', lambda case, line: case, limit=args.limit)
    raise SystemExit(1 if total or errors else 0)


if __name__ == '__main__':
    main()
