#!/usr/bin/env python3
import argparse
import os
import random
import sys
from pathlib import Path

try:
    from . import lab, test_chunk_generation_parity as chunks
except ImportError:
    import lab
    import test_chunk_generation_parity as chunks

DATAPACKS = Path(os.environ.get('VISE_PARITY_DATAPACKS', lab.ROOT / 'tools/fixtures/datapacks'))
SEEDS = [12345, 4242]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seed', type=int, action='append')
    parser.add_argument('--sites', type=int, default=3, help='terrain sites per seed besides 0,0')
    parser.add_argument('--phase', choices=['terrain', 'full'], action='append')
    args = parser.parse_args()
    oracle = chunks.Oracle()
    binary = chunks.rust_binary()
    names = chunks.state_names()
    build = chunks.build_dir() / 'datapacks'
    build.mkdir(exist_ok=True)
    differing = 0
    pack_effect = 0
    for phase in args.phase or ['terrain', 'full']:
        os.environ['VISE_PHASE'] = phase
        for seed in args.seed or SEEDS:
            rng = random.Random(seed)
            sites = [(0, 0)] + [(rng.randint(-300, 300), rng.randint(-300, 300)) for _ in range(args.sites)]
            rects = [f'{x},{z},{x},{z}' for x, z in sites]
            tag = f'{seed}-{phase}'
            vanilla_path, rust_path, plain_path = (build / f'{side}-{tag}.txt' for side in ('vanilla', 'rust', 'plain'))
            os.environ['VISE_DATAPACKS'] = str(DATAPACKS)
            oracle.run(vanilla_path, ['dump', seed, 'overworld', *rects])
            stderr = chunks.run_rust(binary, rust_path, seed, 'overworld', rects)
            del os.environ['VISE_DATAPACKS']
            for line in stderr.splitlines():
                if 'data pack' in line:
                    print('  vise:', line)
            vanilla, rust = chunks.parse(vanilla_path), chunks.parse(rust_path)
            counts = chunks.compare(vanilla, rust, names, f'{tag} packs')
            differing += counts[0]
            print(f'{tag}: {len(vanilla)} chunks with the packs, differing chunks {counts[0]} '
                  f'(blocks {counts[1]}, biome cells {counts[2]}, block entities {counts[3]})', flush=True)
            chunks.run_rust(binary, plain_path, seed, 'overworld', rects)
            plain = chunks.compare(vanilla, chunks.parse(plain_path), names, f'{tag} without packs', quiet=True)
            pack_effect += plain[0]
            print(f'{tag}: without the packs Vise differs from them in {plain[0]} of {len(vanilla)} chunks ({plain[1]} blocks)')
    if not pack_effect:
        sys.exit('the packs changed nothing: the harness would not see them')
    if differing:
        sys.exit(f'{differing} chunks differ from vanilla with the packs')
    print('Data pack worldgen parity passed')


if __name__ == '__main__':
    main()
