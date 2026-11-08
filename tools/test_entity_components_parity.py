#!/usr/bin/env python3
import json
import random

try:
    from . import lab, oracle_harness as harness
except ImportError:
    import lab, oracle_harness as harness

ADVANCEMENTS = ('complete_catalogue', 'whole_pack', 'leash_all_frog_variants')
SPECIES = ('wolf', 'cat', 'frog', 'pig', 'cow', 'chicken')


def predicates():
    folder = lab.RESOURCES / 'data/minecraft/advancement/husbandry'
    out = []
    for name in ADVANCEMENTS:
        data = json.loads((folder / f'{name}.json').read_text())
        for criterion in data['criteria'].values():
            predicate = criterion['conditions']['entity']['predicate']
            out.append(predicate)
            components = {'minecraft:components': predicate['minecraft:components']}
            out.append(components)
            out.append(dict(components, **{'minecraft:entity_type': 'minecraft:pig'}))
    return out


def variants(species):
    folder = lab.RESOURCES / f'data/minecraft/{species}_variant'
    return sorted('minecraft:' + p.stem for p in folder.glob('*.json'))


def make_cases(build):
    path = build / 'cases.jsonl'
    every = predicates()
    rng = random.Random(0x56415249)
    count = 0
    with path.open('w') as out:
        for species in SPECIES:
            for variant in variants(species):
                listed = every + [rng.choice(every) for _ in range(8)]
                case = dict(type='minecraft:' + species, variant=variant, predicates=listed)
                out.write(json.dumps(case, separators=(',', ':')) + '\n')
                count += 1
    return path, count


def main():
    scope = ('The real EntityPredicate of every criterion of complete_catalogue, whole_pack and '
             'leash_all_frog_variants (and their minecraft:components alone, or with another entity type), decoded '
             'with the registries\' ops, on entities made with EntityType.create(LOAD) given each registered variant '
             'of their species through Entity.setComponent. The trigger dispatch around them is not exercised.')
    harness.differential(
        'rust-entity-components-parity', 'VanillaEntityComponentsOracle', 'entity_components_probe', make_cases, scope,
        sources=[lab.ROOT / 'crates/vise-client/src/advancements.rs',
                 lab.ROOT / 'crates/vise-client/src/mobs/finalize.rs'])


if __name__ == '__main__':
    main()
