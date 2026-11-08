#!/usr/bin/env python3
import json
import os
import random
try:
    from . import block_update_harness as h, lab, test_dispenser_parity as d, test_rust_parity as base
except ImportError:
    import block_update_harness as h
    import lab
    import test_dispenser_parity as d
    import test_rust_parity as base


def plain(item, count=1):
    return d.plain(item, count)


def named(item, count, name):
    return '{id:"minecraft:%s",count:%d,components:{"minecraft:custom_name":"%s"}}' % (item, count, name)


STUFF = ['cobblestone', 'diamond', 'stick', 'iron_ingot', 'oak_planks', 'bread', 'arrow', 'redstone', 'ender_pearl', 'torch',
         'iron_sword', 'bow', 'white_wool', 'glass', 'apple']
BOOKS = ['book', 'enchanted_book', 'writable_book', 'written_book', 'knowledge_book']
FOOD = ['porkchop', 'beef', 'chicken', 'mutton', 'rabbit', 'cod', 'salmon', 'potato', 'kelp']
DISCS = ['music_disc_cat', 'music_disc_13', 'music_disc_pigstep', 'music_disc_otherside']

HOLDERS = {
    'chest': (27, STUFF), 'trapped_chest': (27, STUFF), 'barrel': (27, STUFF), 'hopper': (5, STUFF),
    'dispenser': (9, STUFF), 'dropper': (9, STUFF), 'crafter': (9, STUFF), 'furnace': (3, STUFF),
    'blast_furnace': (3, STUFF), 'smoker': (3, FOOD), 'brewing_stand': (5, ['potion', 'blaze_powder', 'nether_wart', 'glass_bottle']),
    'shulker_box': (27, STUFF), 'red_shulker_box': (27, STUFF), 'lime_shulker_box': (27, STUFF),
    'copper_chest': (27, STUFF), 'exposed_copper_chest': (27, STUFF), 'weathered_copper_chest': (27, STUFF),
    'oxidized_copper_chest': (27, STUFF), 'waxed_copper_chest': (27, STUFF), 'waxed_oxidized_copper_chest': (27, STUFF),
    'jukebox': (1, DISCS), 'lectern': (1, ['writable_book', 'written_book']), 'campfire': (4, FOOD), 'soul_campfire': (4, FOOD),
    'decorated_pot': (1, STUFF), 'chiseled_bookshelf': (6, BOOKS), 'oak_shelf': (3, STUFF), 'pale_oak_shelf': (3, STUFF),
    'warped_shelf': (3, STUFF),
    'ender_chest': (0, []), 'enchanting_table': (0, []), 'oak_sign': (0, []), 'white_banner': (0, []), 'beacon': (0, []),
    'bell': (0, []), 'skeleton_skull': (0, []), 'daylight_detector': (0, []), 'comparator': (0, []), 'spawner': (0, []),
}
LISTED = {'chest', 'trapped_chest', 'barrel', 'hopper', 'dispenser', 'dropper', 'crafter', 'furnace', 'blast_furnace',
          'smoker', 'brewing_stand', 'shulker_box', 'red_shulker_box', 'lime_shulker_box', 'copper_chest', 'exposed_copper_chest',
          'weathered_copper_chest', 'oxidized_copper_chest', 'waxed_copper_chest', 'waxed_oxidized_copper_chest', 'jukebox'}
REPLACEMENTS = ['air', 'stone', 'glass', 'water', 'oak_planks', 'barrel', 'chest', 'hopper', 'blue_shulker_box', 'dirt', 'tnt',
                'copper_chest', 'oxidized_copper_chest', 'lectern', 'jukebox']
FLAGS = [3, 3, 3, 2, 18, 2 | 256, 816, 3 | 64, 1]
TOOLS = [plain('diamond_pickaxe'), plain('wooden_pickaxe'), plain('stone_axe'), plain('iron_shovel'), plain('iron_sword'),
         plain('shears'), plain('mace'), '-']


def contents(rng, name, slots, pool):
    items = []
    if not slots:
        return items
    fill = rng.choice([0, 1, 1, 2, 3, slots]) if slots > 1 else rng.choice([0, 1, 1])
    for slot in rng.sample(range(slots), min(fill, slots)):
        item = rng.choice(pool)
        count = 1 if name in ('jukebox', 'lectern', 'chiseled_bookshelf') or item in ('iron_sword', 'bow', 'potion',
                                                                                       'writable_book', 'written_book') else rng.randrange(1, 65)
        if item == 'potion':
            stack = d.potion('potion', rng.choice(['water', 'healing', 'swiftness']))
        elif rng.random() < 0.1 and count > 0:
            stack = named(item, count, 'Kept')
        else:
            stack = plain(item, count)
        items.append([0, 0, 0, slot, stack])
    return items


def holder_state(states, rng, name, items):
    if name == 'lectern':
        return states.get(name, has_book='true' if items else 'false', facing=rng.choice(['north', 'south', 'east', 'west']))
    if name == 'jukebox':
        return states.get(name, has_record='true' if items else 'false')
    if name == 'chiseled_bookshelf':
        filled = {i[3] for i in items}
        return states.get(name, **{f'slot_{i}_occupied': 'true' if i in filled else 'false' for i in range(6)})
    if name in ('chest', 'trapped_chest'):
        return states.get(name, type='single', facing=rng.choice(['north', 'south', 'east', 'west']))
    return states.any(rng, name) if rng.random() < 0.3 else states.get(name)


def other_state(states, rng, name, current):
    choices = [i for i in states.facts.by_block['minecraft:' + name] if i != current]
    return rng.choice(choices) if choices else None


def cases(facts, count, seed, only):
    rng = random.Random(seed)
    states = d.States(facts)
    names = list(HOLDERS)
    for n in range(count):
        name = names[n % len(names)] if rng.random() < 0.7 else rng.choice(names)
        if only and only not in name:
            continue
        slots, pool = HOLDERS[name]
        items = contents(rng, name, slots, pool)
        state = holder_state(states, rng, name, items)
        cells = [[0, 0, 0, state]]
        for _ in range(rng.randrange(0, 4)):
            at = (rng.randrange(-1, 2), rng.randrange(-1, 2), rng.randrange(-1, 2))
            if at != (0, 0, 0):
                h.put(cells, at, states.get(rng.choice(['stone', 'dirt', 'glass', 'oak_planks', 'water', 'comparator', 'redstone_wire'])))
        roll = rng.random()
        if roll < 0.45:
            if name in LISTED and rng.random() < 0.15:
                target = other_state(states, rng, name, state)
                target = target if target is not None else states.get(rng.choice(REPLACEMENTS))
            else:
                target = states.get(rng.choice(REPLACEMENTS))
            op = {'op': 'set', 'state': target, 'flags': rng.choice(FLAGS)}
        elif roll < 0.75:
            op = {'op': 'destroy', 'drop': True}
        elif roll < 0.85:
            op = {'op': 'destroy', 'drop': False}
        elif roll < 0.92:
            op = {'op': 'remove'}
        else:
            op = {'op': 'mine', 'item': TOOLS[int(roll * 1000) % len(TOOLS)]}
        case = {**h.environment(rng), 'mode': 'remove', 'pos': [0, 0, 0], 'cells': cells, 'items': items, 'remove': op}
        case.pop('raining', None)
        case.pop('thundering', None)
        yield case


def describe(case, facts):
    cells = ' '.join(f'{c[0]},{c[1]},{c[2]}={facts.describe(c[3])}' for c in case['cells'])
    op = dict(case['remove'])
    if 'state' in op:
        op['state'] = facts.describe(op['state'])
    return f'{cells} items={json.dumps(case["items"])} remove={json.dumps(op)} seed={case["seed"]}'


def main():
    parser = h.argument_parser(__doc__)
    args = parser.parse_args()
    if args.count == 40000:
        args.count = 4000
    lock, jars, libraries = base.verify_artifacts()
    facts = h.load_facts()
    resources = lab.LOCAL / 'vise-client' / lock['minecraft']['version']
    work = lab.LOCAL / 'block-removal-parity'
    work.mkdir(parents=True, exist_ok=True)
    all_cases = list(cases(facts, args.count, args.seed, args.only))
    (work / 'cases.jsonl').write_text(''.join(json.dumps(case) + '\n' for case in all_cases))
    classes = work / 'classes'
    classes.mkdir(exist_ok=True)
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    java_root = lab.ROOT / 'mods/vise/src/test/java'
    d.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-sourcepath', str(java_root), '-nowarn',
           '-cp', dependencies, '-d', str(classes), str(java_root / 'dev/viselab/parity/VanillaDispenserOracle.java')])
    d.run(['java', '-Xmx3G', '-cp', str(classes) + os.pathsep + dependencies, 'dev.viselab.parity.VanillaDispenserOracle',
           str(resources), str(work / 'cases.jsonl'), str(work / 'native.txt')])
    d.run(['cargo', 'build', '-q', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], cwd=lab.ROOT,
          env={**os.environ, 'CARGO_BUILD_JOBS': os.environ.get('CARGO_BUILD_JOBS', '3')})
    d.run([str(lab.cargo_target() / 'fast/vise-client'), '--resources', str(resources)], cwd=work,
          env=lab.game_env(VISE_DISPENSE_FIXTURE=f'{work / "cases.jsonl"},{work / "rust.txt"}'))
    ignore = tuple(prefix + ' ' for prefix in args.ignore.split(',') if prefix) + ('D ',)
    native = h.split_cases((work / 'native.txt').read_text(), ignore)
    rust = h.split_cases((work / 'rust.txt').read_text(), ignore)
    assert len(native) == len(rust) == len(all_cases), (len(native), len(rust), len(all_cases))
    groups, errors = {}, {}
    for case, a, b in zip(all_cases, native, rust):
        if a and a[0].startswith('error'):
            errors.setdefault(a[0], []).append(case)
            continue
        if not d.same(a, b):
            block = facts.states[case['cells'][0][3]][0].split(':')[1]
            groups.setdefault(f'{block} {case["remove"]["op"]}', []).append((case, a, b))
    for kind, entries in sorted(errors.items(), key=lambda e: -len(e[1])):
        print(f'oracle error x{len(entries)}: {kind}')
    for name, entries in sorted(groups.items(), key=lambda e: -len(e[1])):
        print(f'{name}: {len(entries)} differing')
        for case, a, b in entries[:args.show]:
            print('  case ' + describe(case, facts))
            print('   native ' + ' | '.join(sorted(a)))
            print('   rust   ' + ' | '.join(sorted(b)))
    total = sum(len(v) for v in groups.values())
    skipped = sum(len(v) for v in errors.values())
    print(f'{len(all_cases) - total - skipped}/{len(all_cases) - skipped} cases match ({skipped} oracle errors)')
    (work / 'differences.json').write_text(json.dumps({k: [[c, a, b] for c, a, b in v[:5]] for k, v in groups.items()}))
    h.finish(total + skipped)


if __name__ == '__main__':
    main()
