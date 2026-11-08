#!/usr/bin/env python3
import argparse
import json
try:
    from . import interaction_lab as lib
except ImportError:
    import interaction_lab as lib

POS = 'x:0,y:64,z:0'

BLOCKS = [
    ('chest', 'chest'), ('trapped_chest', 'trapped_chest'),
    ('copper_chest', 'chest'), ('exposed_copper_chest', 'chest'), ('weathered_copper_chest', 'chest'),
    ('oxidized_copper_chest', 'chest'), ('waxed_copper_chest', 'chest'), ('waxed_oxidized_copper_chest', 'chest'),
    ('barrel', 'barrel'), ('shulker_box', 'shulker_box'), ('red_shulker_box', 'shulker_box'),
    ('black_shulker_box', 'shulker_box'), ('dispenser', 'dispenser'), ('dropper', 'dropper'), ('hopper', 'hopper'),
    ('furnace', 'furnace'), ('smoker', 'smoker'), ('blast_furnace', 'blast_furnace'),
    ('brewing_stand', 'brewing_stand'), ('crafter', 'crafter'),
]
NAME = '"minecraft:custom_name"'
VARIANTS = [
    ('', None),
    ('CustomName:"Loot"', None),
    ('', '{%s:"Loot"}' % NAME),
    ('', '{%s:"Box of §cred"}' % NAME),
    ('CustomName:"Old"', '{%s:"New"}' % NAME),
    ('CustomName:"Old"', '{"!minecraft:custom_name":{}}'),
    ('', '{"minecraft:lore":["a"]}'),
]
DOUBLES = [('chest', 'chest'), ('trapped_chest', 'trapped_chest')]
HALVES = [(None, None), ('Right', None), (None, 'Left'), ('Right', 'Left')]


def be(type_id, body=''):
    return '{id:"minecraft:%s",%s%s}' % (type_id, POS, ',' + body if body else '')


def named(type_id, name):
    return be(type_id, 'CustomName:"%s"' % name if name else '')


def cases():
    out = []
    for block, type_id in BLOCKS:
        for body, patch in VARIANTS:
            case = {'block': 'minecraft:' + block, 'nbt': be(type_id, body)}
            if patch is not None:
                case['patch'] = patch
            out.append(case)
    for block, type_id in DOUBLES:
        for first, second in HALVES:
            out.append({'block': 'minecraft:' + block, 'first': named(type_id, first), 'second': named(type_id, second)})
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--limit', type=int, default=4)
    args = parser.parse_args()
    work = lib.work_dir('container-title-parity')
    vanilla = lib.Vanilla(work)
    vanilla.compile('ParityData', 'VanillaContainerTitleOracle')
    all_cases = cases()
    (work / 'cases.jsonl').write_text(''.join(json.dumps(c) + '\n' for c in all_cases), encoding='utf-8')
    vanilla.run('VanillaContainerTitleOracle', work / 'cases.jsonl', work / 'native.txt')
    lib.cargo_example('vise-client', 'container_title_fixture', work / 'cases.jsonl', work / 'rust.txt')
    errors = sum(line == 'error' for line in (work / 'native.txt').read_text(encoding='utf-8').split('\n'))
    if errors:
        print(f'{errors} cases fail in vanilla')
    total = lib.compare(all_cases, work / 'native.txt', work / 'rust.txt',
                        lambda case, line: case['block'] + (' double' if 'first' in case else ''),
                        lambda case: json.dumps(case, ensure_ascii=False), args.limit)
    raise SystemExit(1 if total or errors else 0)


if __name__ == '__main__':
    main()
