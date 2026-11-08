#!/usr/bin/env python3
import argparse
import json
import math
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKS = ['blocks', 'properties', 'sounds', 'shapes', 'drops', 'placement', 'tags', 'recipes', 'items', 'item_stats']
EXAMPLES = 8


def key(k):
    return ','.join(sorted(p for p in k.split(',') if p))


def cells(boxes, grid=32):
    out = set()
    for b in boxes:
        lo = [math.floor(round(v * grid, 6)) for v in b[:3]]
        hi = [math.ceil(round(v * grid, 6)) for v in b[3:]]
        for x in range(lo[0], hi[0]):
            for y in range(lo[1], hi[1]):
                for z in range(lo[2], hi[2]):
                    out.add((x, y, z))
    return out


def same_shape(a, b):
    if a is None or b is None:
        return a == b
    sa, sb = sorted(map(tuple, a)), sorted(map(tuple, b))
    if sa == sb:
        return True
    return cells(a) == cells(b)


class Check:
    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.examples = []

    def record(self, ok, what):
        if ok:
            self.passed += 1
        else:
            self.failed += 1
            if len(self.examples) < EXAMPLES:
                self.examples.append(what)

    def json(self):
        return {'passed': self.passed, 'failed': self.failed, 'examples': self.examples}


def sound_upgrades(oracle, vise):
    out = {}
    for name, hs in vise.get('vanilla_sounds', {}).items():
        ob = oracle['blocks'].get(name)
        if ob:
            os_ = ob['states'].get(ob['default']) or next(iter(ob['states'].values()))
            out.setdefault(json.dumps(os_['sound'], sort_keys=True), []).append(hs)
    return out


def modifiers(entries):
    return sorted((m['type'].replace(':generic.', ':').replace(':player.', ':'), m['id'], round(m['amount'], 4),
                   m['operation'], m['slot'])
                  for m in entries)


def tool_rules(rules):
    blocks = lambda b: tuple(sorted([b] if isinstance(b, str) else b))
    return {blocks(r['blocks']): (r.get('speed'), r.get('correct_for_drops')) for r in rules}


def item_stats(oi, hi):
    c = oi['components']
    yield hi['max_damage'] == c.get('minecraft:max_damage', 0), f'durability {hi["max_damage"]}'
    want = modifiers(oi.get('attribute_modifiers', {}).get('modifiers', []))
    yield modifiers(hi['modifiers']) == want, f'modifiers {modifiers(hi["modifiers"])}, want {want}'
    tool = c.get('minecraft:tool')
    if tool is None:
        yield hi['tool'] is None, 'a tool'
    elif hi['tool'] is None:
        yield False, 'no tool'
    else:
        have = tool_rules(hi['tool']['rules'])
        for blocks, rule in tool_rules(tool['rules']).items():
            yield have.get(blocks) == rule, f'tool rule {blocks}: {have.get(blocks)}, want {rule}'
        yield hi['tool']['default_mining_speed'] == tool.get('default_mining_speed', 1.0), 'default mining speed'
        yield hi['tool']['damage_per_block'] == tool.get('damage_per_block', 1), 'damage per block'
    yield hi['enchantable'] == oi.get('enchantment_value', 0), f'enchantability {hi["enchantable"]}'
    if 'repair_items' in oi:
        yield sorted(hi['repairable']) == sorted(oi['repair_items']), f'repair items {hi["repairable"]}'
    if 'armor' in oi:
        yield hi['equip_slot'] == oi['armor']['slot'], f'armour slot {hi["equip_slot"]}'
    if 'food' in oi:
        yield (hi['food'] or {}).get('nutrition') == oi['food']['nutrition'], f'food {hi["food"]}'


def compare_mod(mod, oracle, vise, carry, upgrades, old_items):
    checks = {c: Check() for c in CHECKS}
    prefix = mod + ':'
    templates = {prefix + b: t for b, t in carry.get('blocks', {}).items()}
    vanilla_tags = {t: set(m) for t, m in oracle['tags']['block'].items() if t.startswith('minecraft:')}
    for name, ob in oracle['blocks'].items():
        if not name.startswith(prefix):
            continue
        hb = vise['blocks'].get(name)
        checks['blocks'].record(hb is not None, f'{name}: not in Vise ({carry.get("blocks_unknown", {}).get(name[len(prefix):], "left out")})')
        if hb is None:
            continue
        for k, os_ in ob['states'].items():
            hs = hb['states'].get(k)
            if hs is None:
                checks['properties'].record(False, f'{name}[{k}]: no such state in Vise')
                continue
            props = [('hardness', os_['hardness'], hs['hardness']),
                     ('explosion_resistance', ob['explosion_resistance'], hs['explosion_resistance']),
                     ('emission', os_['emission'], hs['emission']),
                     ('requires_tool', os_['requires_tool'], hs['requires_tool']),
                     ('friction', ob['friction'], hs['friction']),
                     ('map_color', os_['map_color_id'], hs['map_color_id'])]
            bad = [f'{p} {o} vs {h}' for p, o, h in props
                   if (abs(o - h) > 1e-6 if isinstance(o, float) else o != h)]
            checks['properties'].record(not bad, f'{name}[{k}]: ' + '; '.join(bad))
            upgraded = upgrades.get(json.dumps(os_['sound'], sort_keys=True), [])
            checks['sounds'].record(os_['sound'] == hs['sound'] or hs['sound'] in upgraded,
                                    f'{name}[{k}]: {os_["sound"]["break"]} vs {hs["sound"]["break"]}')
            shapes_ok = same_shape(os_['outline'], hs['outline']) and same_shape(os_['collision'], hs['collision'])
            checks['shapes'].record(shapes_ok, f'{name}[{k}]: outline {os_["outline"]} vs {hs["outline"]}')
            for tool, odrops in os_.get('drops', {}).items():
                hdrops = (hs.get('drops') or {}).get(tool)
                checks['drops'].record(odrops == hdrops, f'{name}[{k}] {tool}: {odrops} vs {hdrops}')
        template = templates.get(name)
        ot = oracle['blocks'].get(template) if template else None
        if ot and 'placement' in ob:
            for face, placed in ob['placement'].items():
                theirs = ot.get('placement', {}).get(face)
                checks['placement'].record(
                    placed is not None and theirs is not None and key(placed) == key(theirs),
                    f'{name} on {face}: {placed} vs {template} {theirs}')
        for tag, members in vanilla_tags.items():
            if name in members:
                checks['tags'].record(name in vise['block_tags'].get(tag, []), f'{name}: not in #{tag}')
    for rid, orec in oracle['recipes'].items():
        if not rid.startswith(prefix):
            continue
        hrec = vise['recipes'].get(rid)
        if hrec is None:
            checks['recipes'].record(False, f'{rid} ({orec["type"]}): not loaded')
            continue
        newer = lambda i: i.startswith('minecraft:') and old_items and i not in old_items
        oing = sorted(sorted(i) for i in orec['ingredients'] if i)
        hing = sorted(sorted(x for x in i if not newer(x)) for i in hrec.get('ingredients', []) if i)
        ok = (orec['result'], orec['count']) == (hrec.get('result'), hrec.get('count')) and oing == hing
        why = f'{rid}: {orec["result"]} x{orec["count"]} vs {hrec.get("result")} x{hrec.get("count")}'
        if oing != hing:
            missing = [i for i in oing if i not in hing][:2]
            why += f'; ingredients differ, e.g. oracle {missing}'
        checks['recipes'].record(ok, why)
    for iid, oi in oracle['items'].items():
        if not iid.startswith(prefix):
            continue
        hi = vise['items'].get(iid)
        checks['items'].record(hi is not None and hi['max_stack_size'] == oi['max_stack_size'],
                               f'{iid}: {"missing" if hi is None else "stack " + str(hi["max_stack_size"])}')
        if hi is not None and 'components' in oi and oi.get('block') is None:
            for ok, what in item_stats(oi, hi):
                checks['item_stats'].record(ok, f'{iid}: {what}')
    result = {c: v.json() for c, v in checks.items()}
    failed = sum(v.failed for v in checks.values())
    total = sum(v.passed + v.failed for v in checks.values())
    return {'mod': mod, 'passed': failed == 0 and total > 0, 'checks': result,
            'blocks_with_differences': len(carry.get('differences', {}))}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--work', required=True)
    parser.add_argument('--resources', default=str(ROOT / '.local/vise-client/26.3'))
    parser.add_argument('--jars', help='export the oracle facts from these jars first')
    parser.add_argument('--deps')
    parser.add_argument('mods', nargs='*')
    args = parser.parse_args()
    work = Path(args.work).resolve()
    facts = work / 'oracle' / 'facts.json'
    if args.jars:
        facts.parent.mkdir(parents=True, exist_ok=True)
        command = [sys.executable, str(ROOT / 'tools/neoforge_oracle.py'), 'export', '--jars', args.jars, '--out', str(facts)]
        if args.deps:
            command += ['--deps', args.deps]
        subprocess.run(command, check=True)
    if not facts.exists():
        sys.exit(f'{facts}: no oracle facts (run with --jars, or tools/neoforge_oracle.py export)')
    subprocess.run(['cargo', 'build', '--release', '-q', '-p', 'vise-client', '--example', 'mod_port_probe'],
                   cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=ROOT, text=True))
    probe = Path(metadata['target_directory']) / 'release/examples/mod_port_probe'
    oracle = json.loads(facts.read_text())
    old_items = set(oracle.get('vanilla_items', []))
    mods = args.mods or sorted(p.name for p in (work / 'mods').iterdir() if (p / 'carry').is_dir())
    rows = []
    for mod in mods:
        out = work / 'mods' / mod / 'harness' / 'vise.json'
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run([str(probe), args.resources, str(work), str(out), mod], check=True)
        vise = json.loads(out.read_text())
        carry_report = work / 'mods' / mod / 'carry-report.json'
        carry = json.loads(carry_report.read_text()) if carry_report.exists() else {}
        rows.append(compare_mod(mod, oracle, vise, carry, sound_upgrades(oracle, vise), old_items))
    print(f'{"mod":<16} {"result":<7} ' + ' '.join(f'{c:>12}' for c in CHECKS))
    for result in rows:
        mod = result['mod']
        (work / 'mods' / mod / 'harness').mkdir(parents=True, exist_ok=True)
        (work / 'mods' / mod / 'harness' / 'result.json').write_text(json.dumps(result, indent=1))
        cols = []
        for c in CHECKS:
            r = result['checks'][c]
            n = r['passed'] + r['failed']
            cols.append(f'{r["passed"]}/{n}' if n else '-')
        print(f'{mod:<16} {"pass" if result["passed"] else "FAIL":<7} ' + ' '.join(f'{c:>12}' for c in cols))


if __name__ == '__main__':
    main()
