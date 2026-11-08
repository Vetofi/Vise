#!/usr/bin/env python3
import base64
import argparse
import gzip
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

try:
    from . import lab
except ImportError:
    import lab

ORACLE = Path(os.environ.get('VISE_STRUCTURE_ORACLE', lab.LOCAL / 'structure-oracle'))
JARS = Path(os.environ.get('VISE_STRUCTURE_JARS', ORACLE / 'jars'))
PACK_JARS = Path(os.environ.get('VISE_STRUCTURE_PACK_JARS', ORACLE / 'pack-jars'))


def jar_path(jar):
    return JARS / jar if (JARS / jar).exists() else PACK_JARS / jar
PROFILES = {
    'lithostitched': {'side': 'base_pieces', 'mods': {}, 'carry': [], 'structures': None},
    'betterdungeons': {'side': 'pieces', 'mods': {'yungsapi': 'YungsApi-1.21.1-NeoForge-5.1.9.jar',
                                                  'betterdungeons': 'YungsBetterDungeons-1.21.1-NeoForge-5.1.4.jar'},
                       'carry': ['betterdungeons'], 'structures': r'^betterdungeons:(small|skeleton|zombie)_dungeon$'},
    **{name: {'side': 'all_starts', 'mods': {'yungsapi': 'YungsApi-1.21.1-NeoForge-5.1.9.jar'},
              'packs': {'probe': name}, 'carry': ['probe'], 'structures': r'^probe:'}
       for name in ['yung-probe', 'yung-probe2']},
}
BUNDLES = {'structure': 'structures', 'structure_set': 'structure_sets', 'template_pool': 'template_pools',
           'processor_list': 'processor_lists'}
TRIAL_CHAMBERS = '26.3 vanilla lays it out as Vise does, unlike 1.21.1'
SULFUR_CAVES = "the start is in 26.3's sulfur_caves, a biome the mod's 1.21.1 tag cannot list"
RUINED_PORTAL = "26.3 vanilla's portal is Vise's; 1.21.1 picked another"
KNOWN = {
    ('lithostitched', 1234, 'minecraft:village_plains', -24, 47): 'no start in 26.3 vanilla (nor in Vise)',
    ('lithostitched', 1234, 'minecraft:ruined_portal', -16, 51): RUINED_PORTAL,
    ('lithostitched', 42, 'minecraft:ruined_portal', -19, -38): RUINED_PORTAL,
    ('betterdungeons', 1234, 'betterdungeons:small_dungeon', 162, 10): SULFUR_CAVES,
    ('betterdungeons', 42, 'betterdungeons:small_dungeon', -29, -28): SULFUR_CAVES,
    **{('lithostitched', seed, 'minecraft:trial_chambers', x, z): TRIAL_CHAMBERS
       for seed, x, z in [(1234, 13, 21), (2, -28, 13), (2, 11, 16), (2, 146, 44), (42, 17, 7), (42, 38, -25)]},
}


def element(saved):
    kind = saved.get('element_type')
    if kind in ('minecraft:single_pool_element', 'minecraft:legacy_single_pool_element', 'yungsapi:yung_single_element'):
        location = saved['location']
        return location if ':' in location else 'minecraft:' + location
    if kind == 'minecraft:feature_pool_element':
        return 'feature:' + saved['feature']
    if kind == 'minecraft:list_pool_element':
        return 'list:' + '+'.join(element(e) for e in saved['elements'])
    if kind == 'minecraft:empty_pool_element':
        return 'empty'
    return kind


def expected_piece(saved):
    box = saved['BB']
    piece = {'type': saved['id'], 'box': box if saved['id'] == 'minecraft:jigsaw' else [box[0], box[2], box[3], box[5]]}
    if saved['id'] == 'minecraft:jigsaw':
        piece.update(element=element(saved['pool_element']), rotation=saved['rotation'],
                     position=[saved['PosX'], saved['PosY'], saved['PosZ']], ground_level_delta=saved['ground_level_delta'])
    return piece


CENSUS = ['blockstates', 'item_models', 'recipes', 'loot_tables', 'tags', 'worldgen', 'structures', 'languages',
          'textures', 'sounds', 'classes', 'mixin_configs']


def pack_jar(work, mod, profile):
    spec = json.loads((ORACLE / 'profiles' / f'{profile}.json').read_text())
    files = {**spec['config_files'], **{path: base64.b64decode(data) for path, data in spec.get('binary_files', {}).items()}}
    jar = work / f'{mod}.jar'
    with zipfile.ZipFile(jar, 'w') as z:
        for path, content in files.items():
            parts = Path(path).parts
            if parts[:2] == ('world', 'datapacks') and len(parts) > 3:
                z.writestr(str(Path(*parts[3:])), content)
    return jar


def carry(profile, mods, carried, packs=None):
    work = lab.LOCAL / 'structure-layout' / profile / 'work'
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    mods = {key: str(jar_path(jar)) for key, jar in mods.items()}
    mods.update({key: str(pack_jar(work, key, name)) for key, name in (packs or {}).items()})
    records = [{'key': key, 'name': key, 'version': '', 'path': jar, 'tier': 'data', 'confidence': 'known',
                'class': 'content' if key in carried else 'library', 'source': 'file', 'license': '',
                'client': '', 'server': '', 'depends': [], 'layer': 0, 'jar': jar,
                'info': {'mod_id': key, 'name': key, 'version': '', 'license': '', 'depends': [],
                         'census': dict.fromkeys(CENSUS, 0)}}
               for key, jar in mods.items()]
    (work / 'inventory.json').write_text(json.dumps(records))
    subprocess.run(['cargo', 'run', '--release', '-q', '-p', 'vise-port-pack', '--', 'carry', str(work),
                    '--resources', str(lab.RESOURCES), *carried], check=True, cwd=lab.ROOT, stdout=subprocess.DEVNULL)
    data = [work / 'mods' / key / 'carry' / 'data' for key in carried]
    if not all(d.is_dir() for d in data):
        raise SystemExit(f'vise-port-pack carried nothing into {work / "mods"}')
    return data


def overlay(profile, data_dirs):
    root = lab.LOCAL / 'structure-layout' / profile / 'resources'
    shutil.rmtree(root, ignore_errors=True)
    worldgen = root / 'worldgen'
    (worldgen / 'structure').mkdir(parents=True)
    (root / 'facts.json').symlink_to(lab.RESOURCES / 'facts.json')
    source = lab.RESOURCES / 'worldgen'
    merged = {name: json.loads((source / f'{name}.json').read_text()) for name in [*BUNDLES.values(), 'biome_tags']}
    additions = []
    for entry in source.iterdir():
        if entry.name != 'structure' and entry.stem not in merged:
            (worldgen / entry.name).symlink_to(entry)
    for namespace in (source / 'structure').iterdir():
        (worldgen / 'structure' / namespace.name).symlink_to(namespace)
    for data in data_dirs:
        for path in sorted(p for p in data.rglob('*') if p.is_file()):
            parts = path.relative_to(data).parts
            if len(parts) < 3:
                continue
            namespace, kind = parts[0], parts[1]
            if kind == 'structure' and path.suffix == '.nbt':
                target = worldgen / 'structure' / namespace / Path(*parts[2:])
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(path)
            elif path.suffix == '.json' and kind == 'worldgen' and parts[2] in BUNDLES:
                text = path.read_text()
                if 'vise:add_to' in text:
                    additions.append((BUNDLES[parts[2]], json.loads(text)))
                else:
                    merged[BUNDLES[parts[2]]][f'{namespace}:' + '/'.join(parts[3:])[:-5]] = text
            elif path.suffix == '.json' and parts[1:4] == ('tags', 'worldgen', 'biome'):
                tag = f'{namespace}:' + '/'.join(parts[4:])[:-5]
                value = json.loads(path.read_text())
                if tag in merged['biome_tags'] and not value.get('replace'):
                    value['values'] = json.loads(merged['biome_tags'][tag])['values'] + value['values']
                merged['biome_tags'][tag] = json.dumps(value)
    for bundle, addition in additions:
        targets = addition['vise:add_to']
        key = 'structures' if bundle == 'structure_sets' else 'elements'
        for target in [targets] if isinstance(targets, str) else targets:
            if target in merged[bundle]:
                value = json.loads(merged[bundle][target])
                value[key] = value.get(key, []) + addition.get(key, [])
                merged[bundle][target] = json.dumps(value)
    for name, bundle in merged.items():
        (worldgen / f'{name}.json').write_text(json.dumps(bundle))
    return root


def probe_binary():
    subprocess.run(['cargo', 'build', '--release', '-q', '-p', 'vise-world', '--example', 'world_probe'], check=True, cwd=lab.ROOT)
    name = 'world_probe.exe' if os.name == 'nt' else 'world_probe'
    return lab.cargo_target() / 'release/examples' / name


def vise_starts(binary, resources, seed, dimension, chunks):
    result = subprocess.run([str(binary), 'structure_pieces', str(resources), str(seed), dimension,
                             *(f'{x},{z}' for x, z in chunks)], capture_output=True, text=True, cwd=lab.ROOT)
    if result.returncode:
        raise SystemExit('structure_pieces failed: ' + result.stderr[-2000:])
    starts, current = {}, None
    for line in result.stdout.splitlines():
        fields = line.split()
        if fields[0] == 'start':
            current = starts.setdefault((fields[1], int(fields[2]), int(fields[3])), [])
        elif fields[0] == 'piece':
            box = list(map(int, fields[8:14]))
            piece = {'type': fields[1], 'box': box if fields[1] == 'minecraft:jigsaw' else [box[0], box[2], box[3], box[5]]}
            if fields[1] == 'minecraft:jigsaw':
                piece.update(element=fields[2], rotation=fields[3], position=list(map(int, fields[4:7])),
                             ground_level_delta=int(fields[7]))
            current.append(piece)
    return starts


def compare(expected, actual):
    for i, (want, got) in enumerate(zip(expected, actual)):
        if want != got:
            return i, {k: (want.get(k), got.get(k)) for k in want.keys() | got.keys() if want.get(k) != got.get(k)}
    if len(expected) != len(actual):
        return min(len(expected), len(actual)), {'pieces': (len(expected), len(actual))}
    return None


def census_starts(oracle):
    for chunk in oracle['all_starts']:
        for start in chunk['starts']:
            if 'children' in start:
                yield {'id': start['id'], 'dimension': chunk['dimension'], 'start_chunk': chunk['chunk'],
                       'all_starts': start['children']}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--profile', default='lithostitched', choices=sorted(PROFILES))
    parser.add_argument('--filter', default='', help='only structure ids matching this regex')
    parser.add_argument('--verbose', action='store_true')
    args = parser.parse_args()
    results = sorted((ORACLE / args.profile).glob('seed-*.json.gz'))
    if not results:
        print(f'SKIP: no oracle results in {ORACLE / args.profile} (see this file\'s docstring)')
        return 0
    profile = PROFILES[args.profile]
    missing = [jar for jar in profile['mods'].values() if not jar_path(jar).exists()]
    missing += [str(ORACLE / 'profiles' / f'{name}.json') for name in profile.get('packs', {}).values()
                if not (ORACLE / 'profiles' / f'{name}.json').exists()]
    if missing:
        print(f'SKIP: not found: {", ".join(missing)}')
        return 0
    resources = lab.RESOURCES
    if profile['carry']:
        data = carry(args.profile, profile['mods'], profile['carry'], profile.get('packs'))
        resources = overlay(args.profile, data)
    binary = probe_binary()
    matched, known, failed = 0, 0, []
    for path in results:
        oracle = json.load(gzip.open(path))
        seed = oracle['seed']
        by_dimension = {}
        for start in oracle['structures'] if profile['side'] != 'all_starts' else census_starts(oracle):
            if start.get(profile['side']) is None or not re.search(args.filter, start['id']) \
                    or not re.search(profile['structures'] or '', start['id']):
                continue
            by_dimension.setdefault(start['dimension'], []).append(start)
        for dimension, starts in by_dimension.items():
            chunks = sorted({tuple(s['start_chunk']) for s in starts})
            actual = vise_starts(binary, resources, seed, dimension.removeprefix('minecraft:'), chunks)
            for start in starts:
                key = (start['id'], *start['start_chunk'])
                expected = [expected_piece(p) for p in start[profile['side']]]
                got = actual.get(key)
                label = f'seed {seed} {start["id"]} chunk {start["start_chunk"]}'
                difference = (0, {'start': 'missing in Vise'}) if got is None else compare(expected, got)
                reason = KNOWN.get((args.profile, seed, *key))
                if difference is None:
                    matched += 1
                    if args.verbose:
                        print(f'ok    {label}: {len(expected)} pieces')
                elif reason:
                    known += 1
                    print(f'known {label}: {reason}')
                else:
                    failed.append(label)
                    index, fields = difference
                    print(f'FAIL  {label}: {len(expected)} pieces expected, {len(got or [])} placed; piece {index}: {fields}')
    total = matched + known + len(failed)
    print(f'{matched} of {total} starts match the oracle piece for piece; {known} known version differences')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
